//! Shared editable controls. Keep slider interactions native to egui, including
//! keyboard stepping, accessible values, typed input, and drag transactions.
use crate::egui;
use crate::tips::Tip as _;
/// Focus only when a dialog opens; subsequent frames must preserve Tab/click navigation.
pub(super) fn focus_once(response: &egui::Response, pending: &mut bool) {
    if std::mem::take(pending) {
        response.request_focus();
    }
}
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

pub(crate) const DIALOG_ACTION_HEIGHT: f32 = 26.;
pub(super) struct DialogButton<'a> {
    button: egui::Button<'a>,
    primary: bool,
}
impl DialogButton<'_> {
    pub fn fill(mut self, color: egui::Color32) -> Self {
        self.button = self.button.fill(color);
        self
    }
    pub fn min_size(mut self, size: egui::Vec2) -> Self {
        self.button = self.button.min_size(size);
        self
    }
}
impl egui::Widget for DialogButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut r = ui.add(self.button);
        let dialog = ui.memory(|m| m.top_modal_layer() == Some(ui.layer_id()))
            || ui.layer_id().order == egui::Order::Foreground;
        if self.primary && dialog && ui.is_enabled() && confirm_key(ui) {
            r.flags.insert(egui::response::Flags::FAKE_PRIMARY_CLICKED);
        }
        r
    }
}
pub(super) fn prepare_dialog_keys(ctx: &egui::Context) {
    let (ime, enter) = ctx.input(|i| {
        (
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::Ime(e) = e {
                        Some(e.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>(),
            i.key_pressed(egui::Key::Enter) && i.modifiers == egui::Modifiers::NONE,
        )
    });
    ctx.data_mut(|d| {
        let id = egui::Id::unique("dialog-ime");
        let mut composing = d.get_temp::<bool>(id).unwrap_or(false);
        for e in &ime {
            match e {
                egui::ImeEvent::Preedit { text, .. } => composing = !text.is_empty(),
                egui::ImeEvent::Commit(_) => composing = false,
                _ => (),
            }
        }
        d.insert_temp(id, composing);
        d.insert_temp(egui::Id::unique("dialog-singleline-enter"), false);
        d.insert_temp(
            egui::Id::unique("dialog-enter"),
            enter && !composing && ime.is_empty(),
        );
    });
}
pub(super) fn confirm_key(ui: &egui::Ui) -> bool {
    if ui.memory(|m| {
        m.top_modal_layer()
            .is_some_and(|layer| layer != ui.layer_id())
    }) {
        return false;
    }
    let singleline = ui.ctx().data(|d| {
        d.get_temp::<bool>(egui::Id::unique("dialog-singleline-enter"))
            .unwrap_or(false)
    });
    if !ui.is_enabled()
        || egui::Popup::is_any_open(ui.ctx())
        || (!singleline
            && (ui.ctx().egui_wants_keyboard_input() || ui.memory(|m| m.focused().is_some())))
    {
        return false;
    }
    let pressed = ui.ctx().data_mut(|d| {
        let id = egui::Id::unique("dialog-enter");
        let value = d.get_temp::<bool>(id).unwrap_or(false);
        d.insert_temp(id, false);
        value
    });
    if pressed {
        ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        });
    }
    pressed
}
pub(super) fn dialog_button(label: &str, primary: bool) -> DialogButton<'_> {
    let b = egui::Button::new(label).min_size(egui::vec2(82., DIALOG_ACTION_HEIGHT));
    let button = if primary {
        b.fill(crate::shell::ACCENT)
    } else {
        b
    };
    DialogButton { button, primary }
}

/// Footer order is primary then secondary, aligned to the dialog's right edge.
pub(super) fn dialog_actions(
    ui: &mut egui::Ui,
    primary: &str,
    secondary: &str,
    enabled: bool,
    destructive: bool,
) -> (bool, bool) {
    ui.add_space(8.);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), DIALOG_ACTION_HEIGHT),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            let cancel = ui
                .add_sized([82., DIALOG_ACTION_HEIGHT], egui::Button::new(secondary))
                .clicked();
            let color = if destructive {
                egui::Color32::from_rgb(177, 43, 49)
            } else {
                crate::shell::ACCENT
            };
            let submit = ui
                .add_enabled(
                    enabled,
                    egui::Button::new(primary)
                        .min_size(egui::vec2(82., DIALOG_ACTION_HEIGHT))
                        .fill(color),
                )
                .clicked();
            (submit || (enabled && confirm_key(ui)), cancel)
        },
    )
    .inner
}

/// Shared context-menu rows keep labels aligned behind a real vector glyph.
pub(crate) fn menu_action(ui: &mut egui::Ui, label: impl AsRef<str>) -> egui::Response {
    let label = label.as_ref();
    let lower = label.to_lowercase();
    let glyph = if lower == "move up" {
        "Move up"
    } else if lower == "move down" {
        "Move down"
    } else if lower == "send to top" {
        "Send to top"
    } else if lower == "send to bottom" {
        "Send to bottom"
    } else if lower.contains("update") || lower.contains("refresh") {
        "Refresh"
    } else if lower.contains("move left") {
        "Move left"
    } else if lower.contains("move right") {
        "Move right"
    } else if lower.contains("unlink") {
        "Unlink"
    } else if lower.contains("paste") {
        "Copy"
    } else if lower.contains("delete") || lower.contains("remove") || lower.contains("clear") {
        "Delete effect"
    } else if lower.contains("split") || lower.contains("separate audio") {
        "Split"
    } else if lower.contains("rename") || lower.contains("text") {
        "Text"
    } else if lower.contains("clipboard") || lower.contains("copy") || lower.contains("duplicate") {
        "Copy"
    } else if lower.contains("library") {
        "Library"
    } else if lower.contains("mute") {
        if lower.contains("unmute") {
            "Audio"
        } else {
            "Muted"
        }
    } else if lower.contains("hide") {
        "Hide annotations"
    } else if lower.contains("show") {
        "Show annotations"
    } else if lower.contains("link") {
        "Link"
    } else if lower.contains("crop") {
        "Crop"
    } else if lower.contains("frame") || lower.contains("picture") || lower.contains("image") {
        "Image"
    } else if lower.contains("track") || lower.contains("front") || lower.contains("back") {
        "Layers"
    } else if lower.contains("fade") || lower.contains("motion") || lower.contains("animation") {
        "Motion"
    } else if lower.contains("file") || lower.contains("import") {
        "Open"
    } else if lower.contains("save") || lower.contains("export") {
        "Save As"
    } else if lower.contains("record") || lower.contains("narration") {
        "Microphone"
    } else if lower.contains("lock") {
        "Lock"
    } else if lower.contains("reset") {
        "Reset effect"
    } else {
        "Properties"
    };
    let text = ui.painter().layout_no_wrap(
        label.into(),
        egui::FontId::proportional(13.),
        ui.visuals().text_color(),
    );
    let r =
        ui.add(egui::Button::new("").min_size(egui::vec2((text.size().x + 42.).max(180.), 28.)));
    crate::video_editor::ui::icon(
        ui.painter(),
        egui::Rect::from_center_size(
            r.rect.left_center() + egui::vec2(14., 0.),
            egui::vec2(16., 16.),
        ),
        glyph,
        ui.visuals().text_color(),
    );
    ui.painter().galley(
        r.rect.left_center() + egui::vec2(30., -text.size().y / 2.),
        text,
        ui.visuals().text_color(),
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    r
}

pub(crate) fn menu_submenu(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    let response = menu_action(ui, label);
    crate::video_editor::ui::icon(
        ui.painter(),
        egui::Rect::from_center_size(
            response.rect.right_center() - egui::vec2(9., 0.),
            egui::vec2(12., 12.),
        ),
        "Expand section",
        ui.visuals().text_color(),
    );
    egui::containers::menu::SubMenu::new().show(ui, &response, content);
}

/// Dialog title, as in the approved capture dialogs (dialog audit, BJ):
/// 20 px bold at the top left; the close button sits at the top right.
pub(crate) fn dialog_title(ui: &mut egui::Ui, text: &str) {
    ui.label(crate::panel_ui::title(text, 20.));
    ui.add_space(6.);
}
/// Dialog section label: small uppercase muted text, with optional help in
/// its tooltip (help never goes in paragraphs in the body).
pub(crate) fn dialog_section(ui: &mut egui::Ui, text: &str, help: Option<&str>) {
    ui.add_space(6.);
    dialog_section_label(ui, text, help);
}
/// The section label alone, for a heading row that also holds a switch.
pub(crate) fn dialog_section_label(ui: &mut egui::Ui, text: &str, help: Option<&str>) {
    use crate::tips::Tip as _;
    let label = ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.)
            .color(egui::Color32::from_rgb(142, 154, 157)),
    );
    if let Some(help) = help {
        label.tip(help);
    }
}
/// Seconds a dialog takes to change height, the same growing and shrinking.
fn dialog_resize_time(ctx: &egui::Context) -> f32 {
    #[cfg(test)]
    if ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("animate-dialogs"))) != Some(true) {
        return 0.;
    }
    ctx.global_style().animation_time
}
/// Application dialogs share a close target and rounded, restrained chrome.
/// Keeping the native ModalResponse preserves focus and input isolation.
pub(crate) struct Modal(egui::Modal);
impl Modal {
    pub(crate) fn new(id: egui::Id) -> Self {
        Self(
            egui::Modal::new(id).frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(48, 56, 59))
                    .stroke(egui::Stroke::new(1.5, crate::shell::ACCENT))
                    .corner_radius(10)
                    .inner_margin(16),
            ),
        )
    }
    pub(crate) fn backdrop_color(mut self, color: egui::Color32) -> Self {
        self.0 = self.0.backdrop_color(color);
        self
    }
    pub(crate) fn area(mut self, area: egui::Area) -> Self {
        self.0 = self.0.area(area);
        self
    }
    pub(crate) fn frame(mut self, frame: egui::Frame) -> Self {
        self.0 = self.0.frame(frame);
        self
    }
    pub(crate) fn show<R>(
        self,
        ctx: &egui::Context,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::ModalResponse<R> {
        self.0.show(ctx, |ui| {
            let origin = ui.next_widget_position();
            let top = origin.y;
            let id = ui.scope_id().with("dialog-height");
            let pass = ui.ctx().cumulative_pass_nr();
            let time = dialog_resize_time(ui.ctx());
            // A dialog settles for its first passes and then animates. Reopened
            // dialogs start at their own size instead of animating to it.
            let continuing = ui
                .ctx()
                .data(|d| d.get_temp::<u64>(id.with("pass")))
                .is_some_and(|last| pass.saturating_sub(last) <= 1);
            let since = if continuing {
                ui.ctx()
                    .data(|d| d.get_temp::<u64>(id.with("since")))
                    .unwrap_or(pass)
            } else {
                pass
            };
            let settled = pass.saturating_sub(since) >= 2 && time > 0.;
            let height = settled
                .then(|| ui.ctx().data(|d| d.get_temp::<f32>(id)))
                .flatten()
                .map(|natural| {
                    ui.ctx()
                        .animate_value_with_time(id.with("shown"), natural, time)
                });
            // A fixed height: the space below a centered dialog moves with the
            // dialog, which would make its content wait for the animation.
            let room = (ui.ctx().content_rect().height() - 64.).max(ui.available_height());
            let area = egui::Rect::from_min_size(origin, egui::vec2(ui.available_width(), room));
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(*ui.layout()));
            if let Some(height) = height {
                // Content past the moving edge is hidden until the dialog reaches it.
                let edge = origin.y + height;
                let mut clip = body.clip_rect();
                clip.max.y = clip.max.y.min(edge + 16.);
                body.set_clip_rect(clip);
            }
            let result = content(&mut body);
            let natural = body.min_rect().size();
            drop(body);
            ui.ctx().data_mut(|d| {
                d.insert_temp(id, natural.y);
                d.insert_temp(id.with("pass"), pass);
                d.insert_temp(id.with("since"), since);
            });
            if !settled {
                ui.ctx()
                    .animate_value_with_time(id.with("shown"), natural.y, 0.);
            }
            ui.allocate_space(egui::vec2(natural.x, height.unwrap_or(natural.y)));
            let rect = egui::Rect::from_min_size(
                egui::pos2(ui.min_rect().right() - 26., top),
                egui::vec2(26., 26.),
            );
            let response = ui
                .interact(
                    rect,
                    ui.scope_id().with("dialog-close"),
                    egui::Sense::click(),
                )
                .tip("Close");
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, 5., egui::Color32::from_white_alpha(20));
            }
            crate::video_editor::ui::icon(
                ui.painter(),
                rect.shrink(6.),
                "Close",
                ui.visuals().text_color(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, "Close dialog")
            });
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("dialog-close"), rect));
            if response.clicked() {
                ui.close();
            }
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_dialog_changes_height_smoothly_both_ways() {
        let ctx = egui::Context::default();
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("animate-dialogs"), true));
        let id = egui::Id::unique("resize-test");
        let run = |tall: bool| {
            let mut height = 0.;
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 800.),
                    )),
                    predicted_dt: 0.016,
                    ..Default::default()
                },
                |ui| {
                    let m = Modal::new(id).show(ui.ctx(), |ui| {
                        ui.set_width(200.);
                        // Content that fills the room offered, as scrolling bodies do.
                        let want = if tall { 400. } else { 100. };
                        ui.allocate_space(egui::vec2(200., ui.available_height().min(want)));
                    });
                    height = m.response.rect.height();
                },
            );
            crate::property_layout::tests::PaintedOutput(out);
            height
        };
        let short = run(false);
        run(false);
        run(true);
        run(true);
        run(true);
        run(true);
        let growing = run(true);
        assert!(growing > short + 1., "{growing} {short}");
        let mut tall = growing;
        for _ in 0..60 {
            tall = run(true);
        }
        assert!(growing < tall - 1., "{growing} {tall}");
        assert!((tall - short - 300.).abs() < 1., "{tall} {short}");
        run(false);
        run(false);
        run(false);
        run(false);
        let shrinking = run(false);
        assert!(
            shrinking < tall - 1. && shrinking > short + 1.,
            "{shrinking}"
        );
        let mut end = shrinking;
        for _ in 0..60 {
            end = run(false);
        }
        assert!((end - short).abs() < 1., "{end} {short}");
        // Growing and shrinking take the same number of frames.
        let frames = |tall_now: bool, goal: f32| {
            (0..80)
                .position(|_| (run(tall_now) - goal).abs() < 0.5)
                .unwrap()
        };
        let grow = frames(true, tall);
        let shrink = frames(false, end);
        assert!(grow.abs_diff(shrink) <= 1, "{grow} {shrink}");
        assert!(grow <= 14, "{grow} frames at 60 fps");
    }
    #[test]
    fn enter_confirms_singleline_dialog_but_not_multiline_invalid_or_composition() {
        for (multiline, enabled, ime, expected) in [
            (false, true, false, true),
            (true, true, false, false),
            (false, false, false, false),
            (false, true, true, false),
        ] {
            let ctx = egui::Context::default();
            let mut value = "Example 📝".to_owned();
            let mut submitted = false;
            for frame in 0..3 {
                let events = if frame == 2 {
                    vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    vec![]
                };
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        prepare_dialog_keys(ui.ctx());
                        if ime {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::unique("dialog-ime"), true);
                                d.insert_temp(egui::Id::unique("dialog-enter"), false);
                            });
                        }
                        crate::controls::Modal::new(egui::Id::unique("enter-test")).show(
                            ui.ctx(),
                            |ui| {
                                let edit = if multiline {
                                    crate::unicode_input::TextEdit::multiline(&mut value)
                                } else {
                                    crate::unicode_input::TextEdit::singleline(&mut value)
                                };
                                let response = ui.add(edit.id(egui::Id::unique("entry")));
                                if frame < 2 {
                                    response.request_focus();
                                }
                                submitted |= dialog_actions(ui, "Save", "Cancel", enabled, false).0;
                            },
                        );
                    },
                );
                output.textures_delta.clear();
            }
            assert_eq!(
                submitted, expected,
                "multiline={multiline}, enabled={enabled}, composing={ime}"
            );
        }
    }
    #[test]
    fn confirmation_actions_keep_26_logical_pixels_at_all_ui_scales() {
        for scale in [1., 1.25, 1.5, 2.] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            ctx.set_pixels_per_point(scale);
            for _ in 0..2 {
                let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                    for label in ["Apply", "Cancel", "Close", "Save and continue", "Print…"] {
                        let r = ui.add(dialog_button(label, label != "Cancel"));
                        assert!(
                            (r.rect.height() - DIALOG_ACTION_HEIGHT).abs() < 0.01,
                            "{label} {scale} {:?}",
                            r.rect
                        );
                    }
                });
                let _ = crate::property_layout::tests::PaintedOutput(out);
            }
        }
    }
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
                        response = Some(toggle(ui, &mut value, "Text stroke"));
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
                "Text stroke",
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
                                "Crop",
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
