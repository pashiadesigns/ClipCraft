//! Approved compact category rail, fixed live preview and shared action footer.
use super::*;
use capture_core::text::{TextFit, TextFont, VerticalAlignment, typography::Case};

const DARK: egui::Color32 = egui::Color32::from_rgb(37, 43, 45);
const PANEL: egui::Color32 = egui::Color32::from_rgb(56, 62, 65);
const RAIL: egui::Color32 = egui::Color32::from_rgb(48, 54, 56);
const MUTED: egui::Color32 = egui::Color32::from_rgb(189, 198, 202);
const TABS: [Tab; 6] = [
    Tab::Font,
    Tab::Paragraph,
    Tab::Styles,
    Tab::Link,
    Tab::Find,
    Tab::Spelling,
];

fn pane(ui: &mut egui::Ui, rect: egui::Rect, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = egui::vec2(8., 6.);
    child.spacing_mut().interact_size.y = 28.;
    add(&mut child);
}
pub(super) fn show(ctx: &egui::Context, state: &mut State) -> (bool, bool) {
    let mut apply = false;
    let mut cancel = false;
    let modal = crate::controls::Modal::new(egui::Id::unique("rich-text-tools"))
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .corner_radius(12)
                .stroke(egui::Stroke::new(2., crate::shell::ACCENT)),
        )
        .show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 32.).clamp(280., 760.);
            let height = (ctx.content_rect().height() - 160.).clamp(190., 500.);
            ui.set_width(width);
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(13.));
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let (header, _) = ui.allocate_exact_size(egui::vec2(width, 48.), egui::Sense::hover());
            ui.painter().rect_filled(
                header,
                egui::CornerRadius {
                    nw: 10,
                    ne: 10,
                    ..Default::default()
                },
                DARK,
            );
            ui.painter().text(
                header.left_center() + egui::vec2(16., 0.),
                egui::Align2::LEFT_CENTER,
                "Text Formatting",
                egui::FontId::proportional(15.),
                egui::Color32::WHITE,
            );
            if width > 500. {
                ui.painter().text(
                    header.right_center() - egui::vec2(16., 0.),
                    egui::Align2::RIGHT_CENTER,
                    scope_label(state),
                    egui::FontId::proportional(12.),
                    MUTED,
                );
            }
            let (body, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
            if width >= 650. {
                let nav = egui::Rect::from_min_max(
                    body.min,
                    egui::pos2(body.left() + 132., body.bottom()),
                );
                let preview =
                    egui::Rect::from_min_max(egui::pos2(body.right() - 222., body.top()), body.max);
                let form = egui::Rect::from_min_max(nav.right_top(), preview.left_bottom());
                navigation(ui, nav, state, false);
                form_pane(ui, form, state);
                preview_pane(ui, preview, state);
                #[cfg(test)]
                record_rect(ctx, "preview", preview);
            } else {
                pane(ui, body, "compact-text-body", |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("text-dialog-responsive")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let w = ui.available_width();
                            let (nav, _) =
                                ui.allocate_exact_size(egui::vec2(w, 120.), egui::Sense::hover());
                            navigation(ui, nav, state, true);
                            let (form, _) =
                                ui.allocate_exact_size(egui::vec2(w, 500.), egui::Sense::hover());
                            form_pane(ui, form, state);
                            let (preview, _) =
                                ui.allocate_exact_size(egui::vec2(w, 400.), egui::Sense::hover());
                            preview_pane(ui, preview, state);
                        });
                });
            }
            state.sync_fields();
            let validation = state.working.validate().map_err(|e| e.to_string());
            if let Err(error) = &validation {
                state.error = error.clone();
            }
            let (footer, _) = ui.allocate_exact_size(egui::vec2(width, 52.), egui::Sense::hover());
            ui.painter().rect_filled(
                footer,
                egui::CornerRadius {
                    sw: 10,
                    se: 10,
                    ..Default::default()
                },
                DARK,
            );
            let buttons = egui::Rect::from_min_size(
                egui::pos2(footer.right() - 184., footer.bottom() - 38.),
                egui::vec2(170., 26.),
            );
            let message = if state.error.is_empty() {
                "Ready to apply"
            } else {
                &state.error
            };
            let message_rect = if width < 500. {
                egui::Rect::from_min_max(
                    footer.min + egui::vec2(14., 4.),
                    egui::pos2(footer.right() - 14., buttons.top() - 2.),
                )
            } else {
                egui::Rect::from_min_max(
                    footer.min + egui::vec2(14., 8.),
                    egui::pos2(buttons.left() - 12., footer.bottom() - 8.),
                )
            };
            // Beside the buttons the message is centered on them (BZ4);
            // above them (narrow) it is centered in the room left.
            let galley = ui.painter().layout(
                message.to_owned(),
                egui::FontId::proportional(12.),
                if validation.is_err() {
                    egui::Color32::LIGHT_RED
                } else {
                    MUTED
                },
                message_rect.width(),
            );
            let center_y = if width < 500. {
                message_rect.center().y
            } else {
                buttons.center().y
            };
            let at = egui::pos2(
                message_rect.left(),
                (center_y - galley.size().y / 2.)
                    .min(message_rect.bottom() - galley.size().y)
                    .max(message_rect.top()),
            );
            #[cfg(test)]
            record_rect(
                ctx,
                "message",
                egui::Rect::from_min_size(at, galley.size()),
            );
            ui.painter()
                .with_clip_rect(message_rect)
                .galley(at, galley, MUTED);
            pane(ui, buttons, "text-actions", |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.;
                    apply = ui
                        .add_enabled(
                            validation.is_ok(),
                            crate::controls::dialog_button("Apply", true),
                        )
                        .clicked();
                    cancel = ui
                        .add(crate::controls::dialog_button("Cancel", false))
                        .clicked();
                });
            });
            #[cfg(test)]
            {
                record_rect(ctx, "body", body);
                record_rect(ctx, "footer", footer);
                record_rect(ctx, "buttons", buttons);
            }
        });
    (apply, cancel || crate::text_edit::cancel_on_escape(&modal))
}
fn scope_label(state: &State) -> &'static str {
    if state.range.is_empty() {
        "Current paragraph"
    } else if state.range.len() == state.working.content.chars().count() {
        "Entire text box"
    } else {
        "Selected text / paragraphs"
    }
}
fn navigation(ui: &mut egui::Ui, rect: egui::Rect, state: &mut State, compact: bool) {
    ui.painter().rect_filled(rect, 0, RAIL);
    pane(ui, rect.shrink(8.), "text-navigation", |ui| {
        let columns = if compact { 2 } else { 1 };
        let width = (ui.available_width() - 5. * (columns - 1) as f32) / columns as f32;
        egui::Grid::new("text-category-grid")
            .num_columns(columns)
            .spacing([5., 5.])
            .show(ui, |ui| {
                for (index, tab) in TABS.into_iter().enumerate() {
                    let (r, response) =
                        ui.allocate_exact_size(egui::vec2(width, 38.), egui::Sense::click());
                    if state.tab == tab || response.hovered() || response.has_focus() {
                        ui.painter().rect_filled(
                            r,
                            5,
                            if state.tab == tab {
                                crate::shell::ACCENT
                            } else {
                                PANEL
                            },
                        );
                    }
                    icon(
                        ui,
                        r.left_center() + egui::vec2(14., 0.),
                        match tab {
                            Tab::Font => "type",
                            Tab::Paragraph => "pilcrow",
                            Tab::Styles => "heading",
                            Tab::Link => "link",
                            Tab::Find => "replace",
                            Tab::Spelling => "spell-check",
                        },
                    );
                    ui.painter().text(
                        r.left_center() + egui::vec2(29., 0.),
                        egui::Align2::LEFT_CENTER,
                        tab.label(),
                        crate::panel_ui::font(12.),
                        egui::Color32::WHITE,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Tab,
                            true,
                            state.tab == tab,
                            tab.label(),
                        )
                    });
                    if response.clicked() {
                        state.sync_fields();
                        state.tab = tab;
                        state.focus = true;
                        state.error.clear();
                    }
                    #[cfg(test)]
                    record_rect(ui.ctx(), tab.label(), r);
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    });
}
fn form_pane(ui: &mut egui::Ui, rect: egui::Rect, state: &mut State) {
    pane(
        ui,
        rect.shrink2(egui::vec2(16., 15.)),
        "text-fields",
        |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("text-fields-scroll", state.tab.label()))
                .max_height(ui.available_height())
                .auto_shrink([false, false])
                .show(ui, |ui| match state.tab {
                    Tab::Font => font_fields(ui, state),
                    Tab::Paragraph => paragraph_fields(ui, state),
                    Tab::Styles => style_fields(ui, state),
                    Tab::Link => link_fields(ui, state),
                    Tab::Find => find_fields(ui, state),
                    Tab::Spelling => spelling_fields(ui, state),
                });
        },
    );
}
fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(8.);
    ui.separator();
    ui.add_space(6.);
    ui.label(egui::RichText::new(label).size(12.).color(MUTED));
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.vertical(|ui| {
        ui.label(label);
        ui.add_sized(
            [ui.available_width(), 28.],
            egui::DragValue::new(value).range(range).speed(0.1),
        );
    });
}
fn paragraph_fields(ui: &mut egui::Ui, state: &mut State) {
    ui.strong("Paragraph layout");
    ui.label(egui::RichText::new("Alignment").color(MUTED).size(12.));
    ui.horizontal(|ui| {
        for (a, label, name) in [
            (Alignment::Left, "Align left", "align-left"),
            (Alignment::Center, "Align center", "align-center"),
            (Alignment::Right, "Align right", "align-right"),
            (Alignment::Justify, "Justify", "align-justify"),
        ] {
            if icon_button(ui, name, label, state.paragraph.alignment == a).clicked() {
                state.paragraph.alignment = a;
            }
        }
    });
    section(ui, "Spacing");
    ui.columns(2, |c| {
        field(
            &mut c[0],
            "Line spacing (×)",
            &mut state.paragraph.line_spacing,
            0.5..=4.,
        );
        field(
            &mut c[1],
            "Tab interval (px)",
            &mut state.paragraph.tab,
            4. ..=512.,
        );
    });
    ui.columns(2, |c| {
        field(
            &mut c[0],
            "Before (px)",
            &mut state.paragraph.before,
            0. ..=512.,
        );
        field(
            &mut c[1],
            "After (px)",
            &mut state.paragraph.after,
            0. ..=512.,
        );
    });
    section(ui, "Indents & lists");
    ui.columns(2, |c| {
        field(
            &mut c[0],
            "Left indent (px)",
            &mut state.paragraph.indent,
            0. ..=512.,
        );
        field(
            &mut c[1],
            "First line (px)",
            &mut state.paragraph.first_indent,
            -512. ..=512.,
        );
    });
    ui.columns(2, |c| {
        c[0].label("List style");
        c[0].horizontal(|ui| super::list_buttons(ui, &mut state.paragraph.list));
        c[1].label("Start at");
        let w = c[1].available_width();
        c[1].add_enabled_ui(state.paragraph.list == List::Numbered, |ui| {
            ui.add_sized(
                [w, 28.],
                egui::DragValue::new(&mut state.paragraph.number).range(1..=999),
            );
        });
    });
    ui.horizontal(|ui| {
        ui.label("List level");
        ui.add(egui::DragValue::new(&mut state.paragraph.level).range(0..=8));
    });
    crate::controls::toggle(
        ui,
        &mut state.paragraph.continue_numbering,
        "Continue previous numbering",
    );
}
fn font_fields(ui: &mut egui::Ui, state: &mut State) {
    ui.strong("Character formatting");
    ui.label("Font family");
    egui::ComboBox::from_id_salt("dialog-font")
        .width(ui.available_width())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .selected_text(state.format.font.label())
        .show_ui(ui, |ui| {
            ui.add(
                crate::unicode_input::TextEdit::singleline(&mut state.font_query)
                    .hint_text("Search installed fonts"),
            );
            for font in [TextFont::UbuntuLight, TextFont::HackRegular]
                .into_iter()
                .chain(capture_core::text::system_fonts::choices())
                .filter(|f| {
                    f.label()
                        .to_lowercase()
                        .contains(&state.font_query.to_lowercase())
                })
            {
                if ui
                    .selectable_label(state.format.font == font, font.label())
                    .clicked()
                {
                    state.format.font = font;
                    ui.close();
                }
            }
        });
    ui.columns(2, |c| {
        field(&mut c[0], "Size (pt)", &mut state.format.size, 6. ..=144.);
        c[1].label("Text color");
        let mut rgba = state
            .format
            .color
            .unwrap_or_else(|| state.target.as_ref().map_or([39, 52, 61, 255], |a| a.color));
        if c[1]
            .color_edit_button_srgba_unmultiplied(&mut rgba)
            .changed()
        {
            state.format.color = Some(rgba);
        }
    });
    ui.horizontal(|ui| {
        for (value, name, label) in [
            (&mut state.format.bold, "bold", "Bold"),
            (&mut state.format.italic, "italic", "Italic"),
            (&mut state.format.underline, "underline", "Underline"),
            (&mut state.format.strike, "strikethrough", "Strikethrough"),
        ] {
            if icon_button(ui, name, label, *value).clicked() {
                *value = !*value;
            }
        }
    });
    section(ui, "Character spacing");
    field(
        ui,
        "Tracking (px)",
        &mut state.format.typography.spacing,
        -4. ..=32.,
    );
    ui.label("Case");
    egui::ComboBox::from_id_salt("text-case")
        .selected_text("As typed")
        .show_ui(ui, |ui| {
            for case in [Case::Upper, Case::Lower, Case::Title] {
                if ui.button(case.label()).clicked() {
                    state.sync_fields();
                    let range = state.range();
                    let text = state
                        .working
                        .content
                        .chars()
                        .skip(range.start)
                        .take(range.len())
                        .collect::<String>();
                    let replacement = case.apply(&text);
                    let count = replacement.chars().count();
                    match editing::replace(&mut state.working, range.clone(), &replacement) {
                        Ok(()) => {
                            state.range = range.start..range.start + count;
                            state.refresh_fields();
                        }
                        Err(e) => state.error = crate::guided::friendly(&e.to_string()),
                    };
                    ui.close();
                }
            }
        });
}
fn style_fields(ui: &mut egui::Ui, state: &mut State) {
    ui.strong("Text styles");
    for preset in Preset::ALL {
        let format = preset.format(state.format.font);
        let family = crate::text_layout::available_family(ui.ctx(), &format.style());
        let (r, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 52.), egui::Sense::click());
        let v = ui.style().interact(&response);
        ui.painter().rect_filled(
            r,
            5,
            if response.hovered() {
                v.bg_fill
            } else {
                egui::Color32::from_rgb(66, 74, 77)
            },
        );
        ui.painter().rect_stroke(
            r,
            5,
            egui::Stroke::new(1., egui::Color32::from_rgb(92, 102, 106)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            r.left_center() + egui::vec2(10., 0.),
            egui::Align2::LEFT_CENTER,
            preset.label(),
            egui::FontId::new(format.size * 4. / 3., family),
            egui::Color32::WHITE,
        );
        // Keep descriptions below the name on narrow forms so the true-size preview never collides.
        let detail = format!(
            "{} pt · {}",
            format.size,
            if format.bold {
                "Bold"
            } else if format.italic {
                "Italic"
            } else {
                "Regular"
            }
        );
        if r.width() > 330. {
            ui.painter().text(
                r.right_center() - egui::vec2(10., 0.),
                egui::Align2::RIGHT_CENTER,
                detail,
                egui::FontId::proportional(11.),
                MUTED,
            );
        } else {
            response.clone().tip(detail);
        }
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, preset.label()));
        #[cfg(test)]
        record_rect(ui.ctx(), preset.label(), r);
        if response.clicked() {
            state.sync_fields();
            let range = state.range();
            preset.apply(&mut state.working, range);
            state.refresh_fields();
            state.error = format!("{} style ready to apply", preset.label());
        }
    }
}
fn link_fields(ui: &mut egui::Ui, state: &mut State) {
    ui.strong("Link");
    ui.label("Selected text");
    let range = state.range();
    let selected = state
        .working
        .content
        .chars()
        .skip(range.start)
        .take(range.len())
        .collect::<String>();
    ui.add(egui::Label::new(selected).wrap());
    ui.label("Destination");
    let r = ui.add(
        crate::unicode_input::TextEdit::singleline(&mut state.link)
            .desired_width(f32::INFINITY)
            .hint_text("https://example.com"),
    );
    crate::controls::focus_once(&r, &mut state.focus);
    if r.changed() {
        state.error.clear();
    }
    section(ui, "Selected words remain clickable in PDF exports.");
    if ui.button("Remove link").clicked() {
        state.link.clear();
        state.error.clear();
    }
    if state.range.is_empty() {
        ui.weak("Select the words to link on the canvas first.");
    }
}
fn find_fields(ui: &mut egui::Ui, state: &mut State) {
    ui.strong("Find & replace");
    ui.label("Find");
    let r = ui.add(
        crate::unicode_input::TextEdit::singleline(&mut state.find).desired_width(f32::INFINITY),
    );
    crate::controls::focus_once(&r, &mut state.focus);
    ui.label("Replace with");
    ui.add(
        crate::unicode_input::TextEdit::singleline(&mut state.replacement)
            .desired_width(f32::INFINITY),
    );
    crate::controls::toggle(ui, &mut state.case, "Match case");
    crate::controls::toggle(ui, &mut state.whole, "Whole words");
    section(ui, "This text box");
    let matches = editing::find(&state.working.content, &state.find, state.case, state.whole);
    ui.label(format!("{} matches", matches.len()));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!matches.is_empty(), egui::Button::new("Replace next"))
            .clicked()
        {
            let result =
                editing::replace(&mut state.working, matches[0].clone(), &state.replacement);
            changed_text(state, result.map(|_| "Replacement ready to apply".into()));
        }
        if ui
            .add_enabled(!matches.is_empty(), egui::Button::new("Replace all"))
            .clicked()
        {
            let result = editing::replace_all(
                &mut state.working,
                &state.find,
                &state.replacement,
                state.case,
                state.whole,
            );
            changed_text(
                state,
                result.map(|n| format!("{n} replacements ready to apply")),
            );
        }
    });
}
fn changed_text(state: &mut State, result: capture_core::Result<String>) {
    match result {
        Ok(message) => {
            state.range = 0..state.working.content.chars().count();
            state.refresh_fields();
            state.spelling.clear();
            state.pending = None;
            state.error = message;
        }
        Err(e) => state.error = crate::guided::friendly(&e.to_string()),
    }
}
fn spelling_fields(ui: &mut egui::Ui, state: &mut State) {
    ui.strong("Spelling");
    ui.label("Language tag");
    let r = ui.add(
        crate::unicode_input::TextEdit::singleline(&mut state.language)
            .hint_text("en-US")
            .desired_width(f32::INFINITY),
    );
    crate::controls::focus_once(&r, &mut state.focus);
    if ui
        .add_enabled(state.pending.is_none(), egui::Button::new("Check spelling"))
        .clicked()
    {
        let text = state.working.content.clone();
        let language = state.language.clone();
        let ctx = ui.ctx().clone();
        let (tx, rx) = std::sync::mpsc::channel();
        state.pending = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(crate::spelling::check(&text, &language));
            ctx.request_repaint();
        });
    }
    if state.pending.is_some() {
        ui.spinner();
    }
    let mut replacement = None;
    for (i, issue) in state.spelling.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(&issue.word);
            for suggestion in &issue.suggestions {
                if ui.button(suggestion).clicked() {
                    replacement = Some((i, suggestion.clone()));
                }
            }
        });
    }
    if let Some((i, value)) = replacement {
        let range = state.spelling[i].range.clone();
        let result = editing::replace(&mut state.working, range, &value);
        changed_text(
            state,
            result.map(|_| "Correction ready. Check spelling again for remaining words.".into()),
        );
    }
    section(ui, "Session corrections");
    crate::controls::toggle(ui, &mut state.autocorrect, "Autocorrect while typing");
    ui.label("One typo=replacement per line");
    ui.add(
        crate::unicode_input::TextEdit::multiline(&mut state.corrections)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .char_limit(2048),
    );
    ui.weak("Uses installed Windows dictionaries. No text is uploaded.");
}
fn preview_pane(ui: &mut egui::Ui, rect: egui::Rect, state: &mut State) {
    ui.painter()
        .rect_filled(rect, 0, egui::Color32::from_rgb(43, 49, 51));
    pane(
        ui,
        rect.shrink2(egui::vec2(13., 14.)),
        "text-preview",
        |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(16., 16.), egui::Sense::hover());
                icon(ui, r.center(), "eye");
                ui.label(egui::RichText::new("Live preview").size(12.).color(MUTED));
            });
            let h = 332_f32.min((rect.height() - 90.).max(90.));
            let (r, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
            ui.painter().rect_filled(r, 5, egui::Color32::WHITE);
            state.sync_fields();
            if state.working.validate().is_ok() {
                let content = r.shrink2(egui::vec2(12., 18.));
                let scale = ui.ctx().pixels_per_point().clamp(1., 4.);
                let size = [
                    content.width().max(1.).round() as u32,
                    content.height().max(1.).round() as u32,
                ];
                let key = format!(
                    "{}:{size:?}:{scale}",
                    serde_json::to_string(&state.working).unwrap_or_default()
                );
                if state.preview.as_ref().is_none_or(|(k, _)| k != &key) {
                    let mut doc = capture_core::Document::blank(size[0], size[1]).unwrap();
                    let mut a = state.target.clone().unwrap();
                    a.kind = crate::Kind::Text;
                    a.start = [0.; 2];
                    a.end = [size[0] as f32, size[1] as f32];
                    a.points.clear();
                    a.image = None;
                    a.callout = None;
                    a.shadow = None;
                    a.rotation_degrees = 0.;
                    a.mirror_x = false;
                    a.fill = None;
                    a.step_outline = None;
                    a.line_style = None;
                    let mut text = state.working.clone();
                    text.vertical = VerticalAlignment::Top;
                    text.padding = 0.;
                    text.fit = TextFit::None;
                    text.wrap = true;
                    a.text = Some(text);
                    doc.annotations.push(a);
                    if let Ok(image) = doc.display_list().and_then(|list| {
                        list.render_viewport(capture_core::display::Viewport {
                            size: [
                                (size[0] as f32 * scale).round() as u32,
                                (size[1] as f32 * scale).round() as u32,
                            ],
                            origin: [0.; 2],
                            scale: scale as f64,
                        })
                    }) {
                        let pixels = egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        );
                        state.preview = Some((
                            key,
                            ui.ctx().load_texture(
                                "text-dialog-preview",
                                pixels,
                                egui::TextureOptions::LINEAR,
                            ),
                        ));
                    }
                }
                if let Some((_, texture)) = &state.preview {
                    ui.painter().image(
                        texture.id(),
                        content,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        egui::Color32::WHITE,
                    );
                }
            }
            ui.label(
                egui::RichText::new(scope_label(state))
                    .size(12.)
                    .color(MUTED),
            );
        },
    );
}
fn icon_button(ui: &mut egui::Ui, name: &str, label: &str, selected: bool) -> egui::Response {
    let response = ui.add_sized([30., 28.], egui::Button::new("").selected(selected));
    icon(ui, response.rect.center(), name);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    response.tip_title(label)
}
fn icon(ui: &egui::Ui, center: egui::Pos2, name: &str) {
    let px = (16. * ui.ctx().pixels_per_point()).round().clamp(16., 128.) as u16;
    let id = egui::Id::unique(("text-dialog-icon", name, px));
    let texture = ui
        .ctx()
        .data_mut(|d| d.get_temp::<egui::TextureHandle>(id))
        .or_else(|| {
            let image = crate::ui_glyphs::render(name, px)?;
            let pixels = egui::ColorImage::from_rgba_premultiplied(
                [px as usize, px as usize],
                image.as_raw(),
            );
            let t = ui.ctx().load_texture(
                format!("text-{name}-{px}"),
                pixels,
                egui::TextureOptions::LINEAR,
            );
            ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
            Some(t)
        });
    if let Some(t) = texture {
        ui.painter().image(
            t.id(),
            egui::Rect::from_center_size(center, egui::vec2(16., 16.)),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
    }
}
#[cfg(test)]
fn record_rect(ctx: &egui::Context, name: &str, r: egui::Rect) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(("text-dialog", name)), r));
}
