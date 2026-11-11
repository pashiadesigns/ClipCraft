use crate::egui;
use crate::tips::Tip as _;
pub(crate) mod eyedropper;
#[cfg(test)]
mod independence_tests;
mod preview;
mod shelf;
pub(super) use preview::Preview;
pub(crate) use shelf::{create_palette, take_image_request};

#[derive(Clone, Default)]
struct ColorDraft {
    original: [u8; 4],
    color: [u8; 4],
    hex: String,
    palette: egui::ecolor::Hsva,
    advanced: bool,
    focus_pending: bool,
}

#[derive(Clone)]
struct LivePopup {
    id: egui::Id,
    rect: egui::Rect,
    swatch: egui::Rect,
}

fn live_popup_key() -> egui::Id {
    egui::Id::unique("live-color-popup")
}

/// Commit before an outside click can switch selection/tool or copy the image.
/// This is read before drawing menus, using the previous frame's popup bounds.
pub(super) fn dismissing_live_popup(ctx: &egui::Context) -> bool {
    if eyedropper::owns_input(ctx) {
        return false;
    }
    let popup = ctx.data_mut(|data| data.get_temp::<LivePopup>(live_popup_key()));
    popup.is_some_and(|popup| {
        let (focused, pressed, pos) = ctx.input(|i| {
            (
                i.focused,
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.interact_pos(),
            )
        });
        egui::Popup::is_id_open(ctx, popup.id)
            && (!focused
                || (pressed
                    && pos.is_some_and(|p| {
                        !popup.rect.contains(p)
                            && !popup.swatch.contains(p)
                            && !shelf::context_contains(ctx, p)
                    })))
    })
}

pub(super) fn close_live_popup(ctx: &egui::Context) {
    eyedropper::cancel(ctx);
    shelf::close_context(ctx);
    if let Some(popup) = ctx.data_mut(|data| {
        let popup = data.get_temp::<LivePopup>(live_popup_key());
        data.remove::<LivePopup>(live_popup_key());
        popup
    }) {
        egui::Popup::close_id(ctx, popup.id);
    }
}

impl ColorDraft {
    fn new(color: [u8; 4]) -> Self {
        Self {
            original: color,
            color,
            hex: hex(color),
            palette: palette_color(color),
            advanced: false,
            focus_pending: true,
        }
    }
    fn set_color(&mut self, color: [u8; 4]) {
        let original = self.original;
        let advanced = self.advanced;
        let focus_pending = self.focus_pending;
        *self = Self::new(color);
        self.original = original;
        self.advanced = advanced;
        self.focus_pending = focus_pending;
    }
}

fn palette_color([r, g, b, a]: [u8; 4]) -> egui::ecolor::Hsva {
    // Initialize RGB independently of alpha: the RGBA convenience constructor
    // passes through 8-bit premultiplication and loses hidden/translucent RGB.
    let mut color = egui::ecolor::Hsva::from_srgb([r, g, b]);
    color.a = f32::from(a) / 255.0;
    color
}

fn hex([r, g, b, a]: [u8; 4]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}

fn parse_hex(text: &str) -> Option<[u8; 4]> {
    let text = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !matches!(text.len(), 6 | 8) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut color = [255; 4];
    for (index, channel) in color.iter_mut().take(text.len() / 2).enumerate() {
        *channel = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(color)
}

fn palette(ui: &mut egui::Ui, draft: &mut ColorDraft, opaque: bool, size: f32) -> [egui::Rect; 3] {
    let mut hsv = egui::ecolor::HsvaGamma::from(draft.palette);
    let before = hsv;
    let mut square = egui::Rect::NOTHING;
    let mut hue = egui::Rect::NOTHING;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(size);
            ui.label("Saturation / brightness");
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());
            square = rect;
            if let Some(p) = response.interact_pointer_pos() {
                hsv.s = ((p.x - square.left()) / square.width()).clamp(0.0, 1.0);
                hsv.v = ((square.bottom() - p.y) / square.height()).clamp(0.0, 1.0);
            }
            response.tip("Choose saturation and brightness");
            // An original mesh keeps visual controls independent of numeric inputs.
            let mut mesh = egui::Mesh::default();
            const N: u32 = 24;
            for y in 0..=N {
                for x in 0..=N {
                    let s = x as f32 / N as f32;
                    let v = 1.0 - y as f32 / N as f32;
                    mesh.colored_vertex(
                        square.min + egui::vec2(s, 1.0 - v) * size,
                        egui::ecolor::HsvaGamma {
                            h: hsv.h,
                            s,
                            v,
                            a: 1.0,
                        }
                        .into(),
                    );
                }
            }
            for y in 0..N {
                for x in 0..N {
                    let i = y * (N + 1) + x;
                    mesh.add_triangle(i, i + 1, i + N + 1);
                    mesh.add_triangle(i + 1, i + N + 2, i + N + 1);
                }
            }
            ui.painter().add(egui::Shape::mesh(mesh));
            let pos = square.min + egui::vec2(hsv.s, 1.0 - hsv.v) * size;
            ui.painter()
                .circle_stroke(pos, 5.0, egui::Stroke::new(3.0, egui::Color32::BLACK));
            ui.painter()
                .circle_stroke(pos, 5.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
        });
        ui.vertical(|ui| {
            ui.set_width(24.0);
            ui.label("Hue");
            hue = gradient_bar(ui, &mut hsv.h, size, true, |h| {
                egui::ecolor::HsvaGamma {
                    h,
                    s: 1.0,
                    v: 1.0,
                    a: 1.0,
                }
                .into()
            });
        });
    });
    let mut alpha = egui::Rect::NOTHING;
    if !opaque {
        ui.label("Opacity");
        let value = hsv;
        alpha = gradient_bar(ui, &mut hsv.a, size + 32.0, false, |a| {
            egui::ecolor::HsvaGamma { a, ..value }.into()
        });
    }
    if hsv != before {
        draft.palette = hsv.into();
        draft.color = draft.palette.to_srgba_unmultiplied();
        draft.hex = hex(draft.color);
    }
    [square, hue, alpha]
}

fn gradient_bar(
    ui: &mut egui::Ui,
    value: &mut f32,
    length: f32,
    vertical: bool,
    color: impl Fn(f32) -> egui::Color32,
) -> egui::Rect {
    let (rect, response) = ui.allocate_exact_size(
        if vertical {
            egui::vec2(18.0, length)
        } else {
            egui::vec2(length, 18.0)
        },
        egui::Sense::click_and_drag(),
    );
    if let Some(p) = response.interact_pointer_pos() {
        *value = if vertical {
            (p.y - rect.top()) / rect.height()
        } else {
            (p.x - rect.left()) / rect.width()
        }
        .clamp(0.0, 1.0);
    }
    egui::color_picker::show_color_at(ui.painter(), egui::Color32::TRANSPARENT, rect);
    let mut mesh = egui::Mesh::default();
    for x in 0..=36 {
        let fraction = x as f32 / 36.0;
        let (a, b) = if vertical {
            let y = rect.top() + rect.height() * fraction;
            (egui::pos2(rect.left(), y), egui::pos2(rect.right(), y))
        } else {
            let x = rect.left() + rect.width() * fraction;
            (egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom()))
        };
        mesh.colored_vertex(a, color(fraction));
        mesh.colored_vertex(b, color(fraction));
        if x < 36 {
            let i = x * 2;
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i + 1, i + 3, i + 2);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    let marker = if vertical {
        egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + rect.height() * *value),
            egui::vec2(rect.width() + 2.0, 4.0),
        )
    } else {
        egui::Rect::from_center_size(
            egui::pos2(rect.left() + rect.width() * *value, rect.center().y),
            egui::vec2(4.0, rect.height() + 2.0),
        )
    };
    ui.painter().rect_stroke(
        marker,
        1.0,
        egui::Stroke::new(3.0, egui::Color32::BLACK),
        egui::StrokeKind::Middle,
    );
    ui.painter().rect_stroke(
        marker,
        1.0,
        egui::Stroke::new(1.0, egui::Color32::WHITE),
        egui::StrokeKind::Middle,
    );
    rect
}

fn advanced(ui: &mut egui::Ui, draft: &mut ColorDraft, opaque: bool) {
    crate::property_layout::section_help(
        ui,
        if opaque { "Exact RGB" } else { "Exact RGBA" },
        "Type channel values from 0 to 255. Alpha 0 is transparent; 255 is opaque. Valid changes apply live.",
    );
    let mut changed = false;
    egui::Grid::new("rgba-channels")
        .num_columns(2)
        .show(ui, |ui| {
            for (index, name) in ["Red", "Green", "Blue", "Alpha"]
                .iter()
                .take(if opaque { 3 } else { 4 })
                .enumerate()
            {
                let label = ui.label(*name);
                changed |= ui
                    .add(egui::DragValue::new(&mut draft.color[index]).range(0..=255))
                    .labelled_by(label.id)
                    .changed();
                ui.end_row();
            }
        });
    if changed {
        draft.set_color(draft.color);
    }
}

fn hex_input(ui: &mut egui::Ui, draft: &mut ColorDraft, opaque: bool) {
    ui.horizontal(|ui| {
        let label = ui.label("Hex");
        let input = ui
            .add(
                crate::unicode_input::TextEdit::singleline(&mut draft.hex)
                    .desired_width(110.0)
                    .char_limit(9),
            )
            .labelled_by(label.id);
        crate::controls::focus_once(&input, &mut draft.focus_pending);
        if input.changed()
            && let Some(parsed) = parse_hex(&draft.hex).filter(|c| !opaque || c[3] == 255)
        {
            draft.color = parsed;
            draft.palette = palette_color(parsed);
        }
        if parse_hex(&draft.hex).is_none_or(|c| opaque && c[3] != 255) {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                if opaque {
                    "Use 6 hex digits, or 8 ending in FF."
                } else {
                    "Enter 6 or 8 hexadecimal digits."
                },
            );
        }
    });
}

fn color_swatch(ui: &mut egui::Ui, color: [u8; 4], label: &str, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("preset-swatch", label)), rect));
    let paint = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    if color[3] == 255 {
        ui.painter().rect_filled(rect.shrink(2.0), 1.0, paint);
    } else {
        egui::color_picker::show_color_at(ui.painter(), paint, rect.shrink(2.0));
    }
    if selected || r.hovered() {
        ui.painter().rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(
                1.5,
                if selected {
                    egui::Color32::LIGHT_BLUE
                } else {
                    egui::Color32::WHITE
                },
            ),
            egui::StrokeKind::Inside,
        );
    }
    r.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    r.tip_title(label)
}

/// Exact byte/hex input never roundtrips through the palette conversion.
/// Normal dismissal retains the last valid live choice.
#[cfg(test)]
pub(super) fn rgba(ui: &mut egui::Ui, color: &mut [u8; 4]) -> egui::Response {
    rgba_options(ui, color, false, false)
}

/// Preview on the actual object. Normal dismissal keeps the last valid color;
/// Escape restores the opening color.
pub(super) fn rgba_live(ui: &mut egui::Ui, color: &mut [u8; 4]) -> egui::Response {
    rgba_options(ui, color, false, false)
}

pub(super) fn rgba_preview(
    ui: &mut egui::Ui,
    color: &mut [u8; 4],
    preview: Preview,
) -> egui::Response {
    rgba_styled(ui, color, false, preview, preview::Layout::Inline)
}

pub(super) fn rgb_preview(
    ui: &mut egui::Ui,
    color: &mut [u8; 3],
    preview: Preview,
) -> egui::Response {
    let mut rgba = [color[0], color[1], color[2], 255];
    let response = rgba_styled(ui, &mut rgba, true, preview, preview::Layout::Inline);
    *color = [rgba[0], rgba[1], rgba[2]];
    response
}

/// Compact reference-style color button with its triangle inside the button.
pub(super) fn property_rgba(
    ui: &mut egui::Ui,
    color: &mut [u8; 4],
    preview: Preview,
) -> egui::Response {
    rgba_styled(ui, color, false, preview, preview::Layout::Property)
}

pub(super) fn canvas_rgba(ui: &mut egui::Ui, color: &mut [u8; 4]) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size = egui::vec2(58.0, 20.0);
        rgba_options(ui, color, true, false)
    })
    .inner
}

fn rgba_options(
    ui: &mut egui::Ui,
    color: &mut [u8; 4],
    canvas: bool,
    opaque: bool,
) -> egui::Response {
    rgba_styled(
        ui,
        color,
        opaque,
        Preview::Fill,
        if canvas {
            preview::Layout::Canvas
        } else {
            preview::Layout::Inline
        },
    )
}

fn rgba_styled(
    ui: &mut egui::Ui,
    color: &mut [u8; 4],
    opaque: bool,
    preview: Preview,
    layout: preview::Layout,
) -> egui::Response {
    let size = if layout == preview::Layout::Property {
        let width = crate::property_layout::swatch_width(ui);
        egui::vec2(width, width + 8.)
    } else if matches!(preview, Preview::Text | Preview::Stroke) {
        ui.spacing().interact_size.max(egui::vec2(44., 24.))
    } else {
        ui.spacing().interact_size
    };
    let (_, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::Role::ColorWell, ui.is_enabled(), preview.label());
        info.current_text_value = Some(hex(*color));
        info
    });
    let popup_id = response.id.with("rgba-popup");
    let draft_id = response.id.with("rgba-draft");
    if !ui.input(|i| i.focused) {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    if response.clicked() && !egui::Popup::is_id_open(ui.ctx(), popup_id) {
        shelf::close_context(ui.ctx());
        ui.ctx()
            .data_mut(|data| data.insert_temp(draft_id, ColorDraft::new(*color)));
    }
    if let Some(mut sampled) = eyedropper::take(ui.ctx(), draft_id) {
        if opaque {
            sampled[3] = 255;
        }
        let mut draft = ui
            .ctx()
            .data_mut(|d| d.get_temp::<ColorDraft>(draft_id))
            .unwrap_or_else(|| ColorDraft::new(*color));
        draft.set_color(sampled);
        ui.ctx().data_mut(|d| d.insert_temp(draft_id, draft));
        if *color != sampled {
            *color = sampled;
            response.mark_changed();
        }
    }
    let child_open = shelf::context_open(ui.ctx()) && egui::Popup::is_id_open(ui.ctx(), popup_id);
    if child_open {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            shelf::close_context(ui.ctx());
        }
        if dismissing_live_popup(ui.ctx()) {
            close_live_popup(ui.ctx());
        }
    }
    let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let popup = egui::Popup::menu(&response)
        .id(popup_id)
        .close_behavior(if child_open || eyedropper::owns_input(ui.ctx()) {
            egui::PopupCloseBehavior::IgnoreClicks
        } else {
            egui::PopupCloseBehavior::CloseOnClickOutside
        })
        .show(|ui| {
            let mut draft = ui
                .ctx()
                .data_mut(|data| data.get_temp::<ColorDraft>(draft_id))
                .unwrap_or_else(|| ColorDraft::new(*color));
            let screen = ui.ctx().content_rect();
            let size = (screen.height() - 340.0)
                .min(screen.width() - 240.0)
                .clamp(160.0, 216.0);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            ui.set_width(if draft.advanced {
                size + 194.0
            } else {
                size + 32.0
            });
            ui.horizontal(|ui| {
                ui.heading("Color");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(draft.advanced, "Advanced")
                        .tip("Show exact RGB and alpha channels")
                        .clicked()
                    {
                        draft.advanced = !draft.advanced;
                    }
                    eyedropper::button(ui, popup_id, draft_id);
                });
            });
            shelf::palette(ui, &mut draft, opaque);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(size + 32.0);
                    palette(ui, &mut draft, opaque, size);
                    hex_input(ui, &mut draft, opaque);
                });
                if draft.advanced {
                    let (divider, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, size + 76.0), egui::Sense::hover());
                    ui.painter().line_segment(
                        [divider.center_top(), divider.center_bottom()],
                        ui.visuals().widgets.noninteractive.bg_stroke,
                    );
                    ui.vertical(|ui| {
                        ui.set_width(144.0);
                        advanced(ui, &mut draft, opaque);
                    });
                }
            });
            shelf::show(ui, &mut draft, opaque);
            let parsed = parse_hex(&draft.hex).filter(|c| !opaque || c[3] == 255);
            ui.horizontal(|ui| {
                ui.label("Preview");
                egui::color_picker::show_color(
                    ui,
                    egui::Color32::from_rgba_unmultiplied(
                        draft.color[0],
                        draft.color[1],
                        draft.color[2],
                        draft.color[3],
                    ),
                    egui::vec2(48.0, 20.0),
                );
            });
            if parsed.is_some() && *color != draft.color {
                *color = draft.color;
                response.mark_changed();
            }
            ui.ctx().data_mut(|data| data.insert_temp(draft_id, draft));
        });
    if let Some(popup) = popup {
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                live_popup_key(),
                LivePopup {
                    id: popup_id,
                    rect: popup.response.rect,
                    swatch: response.rect,
                },
            )
        });
    }
    if !egui::Popup::is_id_open(ui.ctx(), popup_id) {
        ui.ctx().data_mut(|data| {
            if data
                .get_temp::<LivePopup>(live_popup_key())
                .is_some_and(|popup| popup.id == popup_id)
            {
                data.remove::<LivePopup>(live_popup_key());
            }
        });
        let draft = ui
            .ctx()
            .data_mut(|data| data.remove_temp::<ColorDraft>(draft_id));
        // A color kept when the picker closes goes to Recent (bi-proposal 3).
        if !cancelled
            && let Some(draft) = &draft
            && draft.color != draft.original
            && *color == draft.color
        {
            shelf::push_recent(ui.ctx(), draft.color);
        }
        if cancelled
            && let Some(draft) = draft
            && *color != draft.original
        {
            *color = draft.original;
            response.mark_changed();
        }
    }
    preview::paint(
        ui,
        &response,
        *color,
        preview,
        layout,
        egui::Popup::is_id_open(ui.ctx(), popup_id),
    );
    response.tip(format!(
        "{} · choose from the palette or enter exact RGBA channels",
        preview.label()
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn text_rect(
        shapes: &[egui::epaint::ClippedShape],
        label: &str,
    ) -> Option<egui::Rect> {
        fn find(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.galley.rect.translate(t.pos.to_vec2()))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| find(&s.shape, label))
    }

    #[test]
    fn canvas_color_changes_live_and_outside_dismiss_retains_it_without_confirmation() {
        use crate::property_layout::tests::PaintedOutput;
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut color = [20, 50, 80, 128];
        let frame = |color: &mut [u8; 4], events| {
            let mut id = egui::Id::NULL;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    id = canvas_rgba(ui, color).id;
                },
            );
            (id, PaintedOutput(output))
        };
        let (id, _) = frame(&mut color, vec![]);
        egui::Popup::open_id(&ctx, id.with("rgba-popup"));
        frame(&mut color, vec![]);
        let (_, output) = frame(&mut color, vec![]);
        assert!(text_rect(&output.shapes, "Apply color").is_none());
        assert!(text_rect(&output.shapes, "Cancel color").is_none());
        let white = ctx
            .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("preset-swatch", "White"))))
            .unwrap()
            .center();
        let click = |pos, pressed| {
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]
        };
        frame(&mut color, click(white, true));
        frame(&mut color, click(white, false));
        assert_eq!(color, [255; 4]);
        assert!(egui::Popup::is_id_open(&ctx, id.with("rgba-popup")));
        let outside = egui::pos2(780.0, 580.0);
        frame(&mut color, click(outside, true));
        frame(&mut color, click(outside, false));
        assert_eq!(color, [255; 4]);
        assert!(!egui::Popup::is_id_open(&ctx, id.with("rgba-popup")));
    }

    #[test]
    fn palette_opens_fully_visible_advanced_is_optional_and_reopens_simple() {
        use crate::property_layout::tests::{PaintedOutput, dump_meshes};
        use egui::{Event, Modifiers, PointerButton};
        for (canvas, opaque, name) in [
            (false, false, "Object"),
            (true, false, "Canvas"),
            (false, true, "ToolDefault"),
        ] {
            for screen in [egui::vec2(981.0, 687.0), egui::vec2(640.0, 480.0)] {
                let ctx = egui::Context::default();
                crate::shell::configure(&ctx);
                ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.0);
                let original = [19, 85, 160, 255];
                let mut color = original;
                let mut id = egui::Id::NULL;
                let frame = |color: &mut [u8; 4], id: &mut egui::Id, events| {
                    PaintedOutput(ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
                            focused: true,
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                                    egui::pos2(screen.x - 80.0, screen.y - 60.0),
                                    egui::vec2(64.0, 30.0),
                                )),
                                |ui| {
                                    *id = rgba_options(ui, color, canvas, opaque).id;
                                },
                            );
                        },
                    ))
                };
                let mut first = frame(&mut color, &mut id, vec![]);
                egui::Popup::open_id(&ctx, id.with("rgba-popup"));
                let opening = frame(&mut color, &mut id, vec![]);
                first
                    .0
                    .textures_delta
                    .append(opening.textures_delta.clone());
                let output = frame(&mut color, &mut id, vec![]);
                assert!(text_rect(&output.shapes, "Hue").is_some());
                assert_eq!(text_rect(&output.shapes, "Opacity").is_some(), !opaque);
                for numeric in ["Red", "Green", "Blue", "Alpha", "Hex RGB / RGBA", "Palette"] {
                    assert!(
                        text_rect(&output.shapes, numeric).is_none(),
                        "default {name}: {numeric}"
                    );
                }
                // Inspect real visual meshes, not just control presence.
                fn check_mesh(
                    shape: &egui::Shape,
                    clip: egui::Rect,
                    screen: egui::Rect,
                    count: &mut usize,
                ) {
                    match shape {
                        egui::Shape::Mesh(m) if matches!(m.vertices.len(), 625 | 74) => {
                            let rect = m.calc_bounds();
                            assert!(
                                screen.contains_rect(rect),
                                "picker outside screen: {rect:?}"
                            );
                            assert!(
                                clip.contains_rect(rect),
                                "picker clipped: {rect:?} vs {clip:?}"
                            );
                            *count += 1;
                        }
                        egui::Shape::Vec(shapes) => {
                            for s in shapes {
                                check_mesh(s, clip, screen, count);
                            }
                        }
                        _ => {}
                    }
                }
                let mut count = 0;
                for shape in &output.shapes {
                    check_mesh(
                        &shape.shape,
                        shape.clip_rect,
                        egui::Rect::from_min_size(egui::Pos2::ZERO, screen),
                        &mut count,
                    );
                }
                assert_eq!(count, if opaque { 2 } else { 3 });
                assert_eq!(color, original);
                let tab = text_rect(&output.shapes, "Advanced").unwrap().center();
                if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_PALETTE_DIR") {
                    dump_meshes(
                        &ctx,
                        &root,
                        &format!("{name}-{}-simple", screen.y),
                        PaintedOutput(first.0.clone()),
                        output,
                    );
                }
                let click = |pos, pressed| {
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            pressed,
                            button: PointerButton::Primary,
                            modifiers: Modifiers::NONE,
                        },
                    ]
                };
                let press = frame(&mut color, &mut id, click(tab, true));
                first.0.textures_delta.append(press.textures_delta.clone());
                let release = frame(&mut color, &mut id, click(tab, false));
                first
                    .0
                    .textures_delta
                    .append(release.textures_delta.clone());
                let advanced = frame(&mut color, &mut id, vec![]);
                for label in ["Red", "Green", "Blue", "Hue"] {
                    assert!(
                        text_rect(&advanced.shapes, label).is_some(),
                        "{name} {label}"
                    );
                }
                assert_eq!(text_rect(&advanced.shapes, "Alpha").is_some(), !opaque);
                for shape in &advanced.shapes {
                    check_mesh(
                        &shape.shape,
                        shape.clip_rect,
                        egui::Rect::from_min_size(egui::Pos2::ZERO, screen),
                        &mut count,
                    );
                }
                assert_eq!(color, original, "switching tabs must not change color");
                if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_PALETTE_DIR") {
                    dump_meshes(
                        &ctx,
                        &root,
                        &format!("{name}-{}-advanced", screen.y),
                        first,
                        advanced,
                    );
                }
                egui::Popup::close_id(&ctx, id.with("rgba-popup"));
                frame(&mut color, &mut id, vec![]);
                // A real swatch click starts a fresh simple view.
                let swatch = egui::pos2(screen.x - 70.0, screen.y - 50.0);
                frame(&mut color, &mut id, click(swatch, true));
                frame(&mut color, &mut id, click(swatch, false));
                let reopened = frame(&mut color, &mut id, vec![]);
                assert!(text_rect(&reopened.shapes, "Hue").is_some());
                assert!(text_rect(&reopened.shapes, "Red").is_none());
            }
        }
    }

    #[test]
    fn visual_picker_drags_change_live_color_and_numeric_edits_keep_advanced_open() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut draft = ColorDraft::new([240, 40, 40, 200]);
        let frame = |draft: &mut ColorDraft, events| {
            let mut rects = [egui::Rect::NOTHING; 3];
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 500.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    rects = palette(ui, draft, false, 236.0);
                },
            );
            output.textures_delta.clear();
            rects
        };
        let rects = frame(&mut draft, vec![]);
        let click = |pos, pressed| {
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    pressed,
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                },
            ]
        };
        let square = rects[0];
        let start = square.min + egui::vec2(70.0, 70.0);
        let end = square.min + egui::vec2(210.0, 25.0);
        frame(&mut draft, click(start, true));
        let pressed = draft.color;
        frame(&mut draft, vec![Event::PointerMoved(end)]);
        assert_ne!(draft.color, pressed);
        let dragged = draft.color;
        frame(&mut draft, click(end, false));
        assert_eq!(draft.color, dragged);
        assert_eq!(draft.color[3], 200);
        assert!(rects[1].height() > rects[1].width());
        let green = rects[1].min + egui::vec2(9.0, rects[1].height() / 3.0);
        frame(&mut draft, click(green, true));
        frame(&mut draft, click(green, false));
        assert!(draft.color[1] > draft.color[0] && draft.color[1] > draft.color[2]);
        let translucent = rects[2].center();
        frame(&mut draft, click(translucent, true));
        frame(&mut draft, click(translucent, false));
        assert!((127..=128).contains(&draft.color[3]));
        assert_eq!(parse_hex(&draft.hex), Some(draft.color));
        draft.advanced = true;
        draft.set_color([17, 85, 201, 0]);
        assert!(draft.advanced);
        assert_eq!(draft.color, [17, 85, 201, 0]);
    }

    #[test]
    fn live_popup_previews_exact_bytes_and_normal_dismiss_keeps_color() {
        let ctx = egui::Context::default();
        let original = [19, 85, 160, 200];
        let mut color = original;
        let mut id = egui::Id::NULL;
        let frame = |color: &mut [u8; 4], id: &mut egui::Id, focused, escape| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused,
                    events: if escape {
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }]
                    } else {
                        vec![]
                    },
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    *id = rgba_live(ui, color).id;
                },
            );
            output.textures_delta.clear();
        };
        frame(&mut color, &mut id, true, false);
        for (focus_loss, escape) in [(false, false), (true, false), (false, true)] {
            let mut draft = ColorDraft::new(original);
            draft.set_color([9, 200, 117, 0]);
            assert_eq!(draft.original, original);
            ctx.data_mut(|data| data.insert_temp(id.with("rgba-draft"), draft));
            egui::Popup::open_id(&ctx, id.with("rgba-popup"));
            frame(&mut color, &mut id, true, false);
            assert_eq!(color, [9, 200, 117, 0]);
            if !focus_loss && !escape {
                egui::Popup::close_id(&ctx, id.with("rgba-popup"));
            }
            frame(&mut color, &mut id, !focus_loss, escape);
            assert_eq!(color, if escape { original } else { [9, 200, 117, 0] });
            assert!(!egui::Popup::is_any_open(&ctx));
        }
    }

    #[test]
    fn live_palette_outside_click_commits_before_tool_or_selection_change_once() {
        use egui::{Event, Modifiers, PointerButton};
        for (target, fill, batch) in [
            ("canvas", false, false),
            ("selection", true, false),
            ("tool", false, false),
            ("focus_loss", true, false),
            ("canvas", true, true),
        ] {
            let ctx = egui::Context::default();
            let mut editor = crate::canvas::tests::editor(&ctx);
            let fixture: capture_core::Document = serde_json::from_str(include_str!(
                "../../../crates/core/tests/fixtures/properties.capture.json"
            ))
            .unwrap();
            let object = fixture
                .annotations
                .iter()
                .find(|a| a.kind == crate::Kind::Rectangle)
                .unwrap()
                .clone();
            let id = object.id;
            let mut other = object.clone();
            other.id = id + 100;
            editor.session.document.annotations = vec![object.clone(), other];
            editor.select_only(Some(id));
            if batch {
                editor.click_selection(Some(id + 100), true);
            }
            let original = editor.session.document.annotations.clone();
            let mut swatch = egui::Id::NULL;
            let mut frame = |editor: &mut crate::Editor,
                             events: Vec<Event>,
                             focused: bool,
                             change_target: bool| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 700.0),
                        )),
                        focused,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        editor.prepare_property_input(&ctx);
                        if change_target && matches!(target, "selection" | "tool") {
                            editor.cancel_gesture();
                            if target == "selection" {
                                editor.select_only(Some(id + 100));
                            } else {
                                editor.tool = Some(crate::Kind::Line);
                            }
                        }
                        let mut edited = editor.selected_annotation().unwrap();
                        swatch = if fill {
                            rgba_live(ui, edited.fill.as_mut().unwrap()).id
                        } else {
                            rgba_live(ui, &mut edited.color).id
                        };
                        editor.preview_property_edit(edited);
                        editor.settle_property_preview(&ctx);
                    },
                );
                output.textures_delta.clear();
                swatch
            };
            let swatch = frame(&mut editor, vec![], true, false);
            let initial_color = if fill {
                object.fill.unwrap()
            } else {
                object.color
            };
            let mut draft = ColorDraft::new(initial_color);
            let chosen = [19, 85, 160, 200];
            draft.set_color(chosen);
            ctx.data_mut(|data| data.insert_temp(swatch.with("rgba-draft"), draft));
            egui::Popup::open_id(&ctx, swatch.with("rgba-popup"));
            frame(&mut editor, vec![], true, false);
            let shown = editor.selected_annotation().unwrap();
            assert_eq!(
                if fill {
                    shown.fill.unwrap()
                } else {
                    shown.color
                },
                chosen
            );
            assert_eq!(editor.session.document.annotations, original);
            assert!(!editor.session.can_undo());
            let outside = egui::pos2(800.0, 650.0);
            let button = |pressed| Event::PointerButton {
                pos: outside,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            frame(
                &mut editor,
                if target == "focus_loss" {
                    vec![]
                } else {
                    vec![Event::PointerMoved(outside), button(true)]
                },
                target != "focus_loss",
                true,
            );
            frame(&mut editor, vec![button(false)], true, false);
            assert_eq!(
                if fill {
                    editor.session.document.annotations[0].fill.unwrap()
                } else {
                    editor.session.document.annotations[0].color
                },
                chosen,
                "{target}"
            );
            if batch {
                assert_eq!(editor.session.document.annotations[1].fill, Some(chosen));
            } else {
                assert_eq!(editor.session.document.annotations[1], original[1]);
            }
            assert!(editor.session.can_undo());
            let reopened: capture_core::Document =
                serde_json::from_slice(&serde_json::to_vec(&editor.session.document).unwrap())
                    .unwrap();
            assert_eq!(
                reopened.render().unwrap(),
                editor.session.document.render().unwrap()
            );
            editor.execute(crate::Command::Undo);
            assert_eq!(editor.session.document.annotations, original);
            assert!(!editor.session.can_undo());
        }
    }

    #[test]
    fn exact_hex_roundtrips_all_byte_values_including_hidden_rgb() {
        for channel in 0..=255 {
            for alpha in [0, 1, 128, 255] {
                let color = [channel, 255 - channel, 171, alpha];
                assert_eq!(parse_hex(&hex(color)), Some(color));
                assert_eq!(ColorDraft::new(color).color, color);
            }
        }
        assert_eq!(parse_hex("  #19aBf0  "), Some([25, 171, 240, 255]));
        assert_eq!(parse_hex("19ABF000"), Some([25, 171, 240, 0]));
        for invalid in [
            "",
            "#",
            "#fff",
            "1234567",
            "123456789",
            "#12ggffff",
            "é2345678",
            "12 45678",
        ] {
            assert_eq!(parse_hex(invalid), None);
        }
    }

    #[test]
    fn displaying_translucent_and_invisible_colors_preserves_exact_rgba() {
        for rgb in [[25, 85, 160], [159, 34, 45], [255, 1, 128]] {
            for alpha in [0, 1, 64, 128, 200, 255] {
                let original = [rgb[0], rgb[1], rgb[2], alpha];
                let mut color = original;
                let ctx = egui::Context::default();
                let mut palette_draft = ColorDraft::new(original);
                let mut popup = None;
                for _ in 0..3 {
                    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                        let response = rgba(ui, &mut color);
                        assert!(!response.changed());
                        popup = Some(response.id.with("rgba-popup"));
                        palette(ui, &mut palette_draft, false, 220.0);
                    });
                    // This headless widget check intentionally has no GPU.
                    output.textures_delta.clear();
                    assert_eq!(color, original);
                    assert_eq!(palette_draft.color, original);
                    egui::Popup::open_id(&ctx, popup.unwrap());
                }
            }
        }
    }
}
