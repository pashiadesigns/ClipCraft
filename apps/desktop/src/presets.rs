use crate::tips::Tip as _;
use crate::{Command, Editor, Kind, egui};
use capture_core::styles::{ArrowStyle, Store};

pub(crate) struct Presets {
    pub(crate) reusable: crate::reusable_presets::State,
    store: Option<Store>,
    error: String,
    name: String,
    pub(crate) alpha: u8,
    pub(super) templates: Option<capture_core::object_styles::Store>,
    pub(super) template_error: String,
    pub(super) template_name: String,
    pub(super) template_chosen: Option<(Kind, String, bool)>,
    pub(super) drawing_styles: Vec<capture_core::object_styles::ObjectStyle>,
}
impl Default for Presets {
    fn default() -> Self {
        Self {
            reusable: Default::default(),
            store: None,
            error: String::new(),
            name: "Custom arrow".into(),
            alpha: 255,
            templates: None,
            template_error: String::new(),
            template_name: String::new(),
            template_chosen: None,
            drawing_styles: vec![],
        }
    }
}
impl Presets {
    /// Saved arrow styles, for kits.
    pub(crate) fn arrow_store(&mut self) -> Option<&mut Store> {
        self.store.as_mut()
    }
    pub(crate) fn new() -> Self {
        let root =
            crate::storage_locations::configured(capture_core::quick_preferences::Folder::Presets)
                .or_else(|| {
                    std::env::var_os("CLIPDESIGNER_STYLE_DIR").map(std::path::PathBuf::from)
                })
                .or_else(|| {
                    std::env::var_os("LOCALAPPDATA").map(|p| {
                        std::path::PathBuf::from(p)
                            .join("ClipDesigner")
                            .join("styles-v1")
                    })
                });
        let mut result = match root.as_ref().map(|r| Store::open(r)) {
            Some(Ok(store)) => Self {
                store: Some(store),
                ..Self::default()
            },
            Some(Err(e)) => Self {
                error: e.to_string(),
                ..Self::default()
            },
            None => Self {
                error: "Local style storage is unavailable.".into(),
                ..Self::default()
            },
        };
        match root.map(|r| capture_core::object_styles::Store::open(&r)) {
            Some(Ok(store)) => result.templates = Some(store),
            Some(Err(error)) => result.template_error = error.to_string(),
            None => result.template_error = "Local template storage is unavailable.".into(),
        }
        result
    }
}
impl Editor {
    pub(super) fn presets_header(
        &mut self,
        ui: &mut egui::Ui,
        tool: Option<Kind>,
    ) -> egui::Response {
        let header = crate::property_layout::section(ui, "Presets");
        crate::property_layout::help_at(
            ui,
            crate::property_layout::beside_title(ui, header, "Presets"),
            "Presets",
            "Select an object to view its formatting templates. Click a style to apply it; the triangle opens variations. The active style is outlined. Use Save current (or +) to save the selected formatting. Right-click a saved style to update or delete it.",
        );
        let rect = egui::Rect::from_center_size(
            header.right_center() - egui::vec2(16.0, 0.0),
            egui::vec2(24.0, 24.0),
        );
        // BY11: Manage presets on the header row, left of the +.
        if crate::manage_presets::header_link(ui, header, 36.0) {
            self.manage_presets_from(ui.ctx());
        }
        self.preset_save_button(ui, tool, Some(rect))
    }
    /// The "+" that saves the selected object's formatting as a preset. The
    /// redesigned property panes place it in their Presets section (AX4).
    pub(crate) fn preset_save_button(
        &mut self,
        ui: &mut egui::Ui,
        tool: Option<Kind>,
        rect: Option<egui::Rect>,
    ) -> egui::Response {
        // A drawn plus stays centered in its button at every scale (BB8).

        let button = egui::Button::new("");
        let response = match rect {
            Some(rect) => ui.put(rect, button),
            None => ui.add_sized([24.0, 24.0], button),
        }
        .tip("Save custom style");
        let c = response.rect.center();
        let plus = egui::Stroke::new(1.6, ui.visuals().text_color());
        ui.painter()
            .line_segment([c - egui::vec2(5.5, 0.0), c + egui::vec2(5.5, 0.0)], plus);
        ui.painter()
            .line_segment([c - egui::vec2(0.0, 5.5), c + egui::vec2(0.0, 5.5)], plus);
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("preset-save-button"), response.rect));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Save custom style")
        });
        self.preset_save_popup(ui, &response, tool);
        response
    }
    /// The name-and-save popup for the selected object's formatting, opened by
    /// the "Save current" preset tile (approved option B) or the + button.
    pub(crate) fn preset_save_popup(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        tool: Option<Kind>,
    ) {
        let kind = self.selected_annotation().map(|a| a.kind).or(tool);
        let arrow = kind == Some(Kind::Arrow);
        let objects = self.selected_objects();
        let available = if arrow {
            self.presets.store.is_some() && objects.iter().all(|a| a.kind == Kind::Arrow)
        } else {
            objects.len() == 1 && self.presets.templates.is_some()
        };
        egui::Popup::menu(response)
            .id(response.id.with("save-custom-style"))
            .frame(egui::Frame::popup(ui.style()).inner_margin(14))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(240.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.spacing_mut().button_padding = egui::vec2(12.0, 6.0);
                let label = ui
                    .horizontal(|ui| {
                        let r = ui.label("Preset name");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::video_editor::ui::button(ui, "Close", "", 26., false)
                                .tip("Close")
                                .clicked()
                            {
                                ui.close();
                            }
                        });
                        r
                    })
                    .inner;
                let name = if arrow {
                    &mut self.presets.name
                } else {
                    &mut self.presets.template_name
                };
                let input = ui
                    .add(
                        crate::unicode_input::TextEdit::singleline(name)
                            .char_limit(40)
                            .desired_width(240.0)
                            .hint_text("Optional name"),
                    )
                    .labelled_by(label.id);
                if response.clicked() {
                    input.request_focus();
                }
                ui.label("Compatible presets");
                let names: Vec<String> = if arrow {
                    self.presets
                        .store
                        .as_ref()
                        .map(|s| s.entries().iter().map(|e| e.name.clone()).collect())
                        .unwrap_or_default()
                } else {
                    self.presets
                        .templates
                        .as_ref()
                        .map(|s| {
                            s.entries()
                                .iter()
                                .filter(|e| {
                                    objects.first().is_some_and(|a| {
                                        a.kind == e.kind && a.focus.is_some() == e.focus.is_some()
                                    })
                                })
                                .map(|e| e.name.clone())
                                .collect()
                        })
                        .unwrap_or_default()
                };
                egui::ScrollArea::vertical()
                    .id_salt("save-object-presets")
                    .max_height(180.)
                    .show(ui, |ui| {
                        if ui
                            .selectable_label(name.is_empty(), "+ New preset")
                            .clicked()
                        {
                            name.clear();
                            input.request_focus();
                        }
                        for preset in names {
                            if ui
                                .selectable_label(name.eq_ignore_ascii_case(&preset), &preset)
                                .clicked()
                            {
                                *name = preset;
                            }
                        }
                    });
                ui.weak("Select an existing preset to update, or enter a new name.");
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        available,
                        egui::Button::new("Save Style")
                            .fill(crate::shell::ACCENT)
                            .min_size(egui::vec2(240.0, 30.0)),
                    )
                    .tip_disabled("Select one object with available local style storage.")
                    .clicked()
                {
                    let saved = if arrow {
                        self.save_arrow_style()
                    } else {
                        self.save_selected_template()
                    };
                    if saved {
                        ui.ctx().request_repaint();
                        ui.close();
                    }
                }
                let error = if arrow {
                    &self.presets.error
                } else {
                    &self.presets.template_error
                };
                if !error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            });
    }
    fn save_arrow_style(&mut self) -> bool {
        let name = if self.presets.name.trim().is_empty() {
            let mut n = 1;
            loop {
                let name = format!("Arrow {n}");
                if self.presets.store.as_ref().is_none_or(|s| {
                    !s.entries()
                        .iter()
                        .any(|s| s.name.eq_ignore_ascii_case(&name))
                }) {
                    break name;
                }
                n += 1;
            }
        } else {
            self.presets.name.trim().to_owned()
        };
        let style = self.arrow_style(name);
        let Some(store) = &mut self.presets.store else {
            return false;
        };
        match store.save(style) {
            Ok(()) => {
                self.presets.error.clear();
                self.status = "Arrow style saved locally. Its tile is ready to use.".into();
                true
            }
            Err(error) => {
                self.presets.error = crate::guided::friendly(&error.to_string());
                false
            }
        }
    }
    fn arrow_style(&self, name: String) -> ArrowStyle {
        if let Some(a) = self.selected_annotation().filter(|a| a.kind == Kind::Arrow) {
            ArrowStyle {
                shadow: a.shadow,
                line_style: a.line_style,
                name,
                color: a.color,
                width: a.width,
            }
        } else {
            ArrowStyle {
                line_style: self
                    .new_annotation(Kind::Arrow, [20., 20.], [80., 60.])
                    .line_style,
                shadow: self
                    .new_annotation(Kind::Arrow, [20., 20.], [80., 60.])
                    .shadow,
                name,
                color: [
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    self.presets.alpha,
                ],
                width: self.width,
            }
        }
    }
    pub(crate) fn apply_arrow_style(&mut self, style: &ArrowStyle) {
        if let Err(e) = style.validate() {
            self.fail(e);
            return;
        }
        if let Some(a) = self.selected_annotation() {
            if a.kind != Kind::Arrow {
                self.status = "Select an arrow to apply an arrow style.".into();
                return;
            }
            let annotations = self
                .selected_objects()
                .into_iter()
                .filter(|a| a.kind == Kind::Arrow)
                .map(|mut a| {
                    a.color = style.color;
                    a.width = style.width;
                    a.shadow = style.shadow;
                    a.line_style = style.line_style;
                    a
                })
                .collect();
            self.execute(Command::UpdateMany { annotations });
        }
        self.color = style.color[..3].try_into().expect("three color bytes");
        self.width = style.width;
        self.presets.alpha = style.color[3];
        let mut example = self.new_annotation(Kind::Arrow, [20.0, 20.0], [80.0, 60.0]);
        example.shadow = style.shadow;
        example.line_style = style.line_style;
        self.remember_annotation_style(&example);
        self.status = format!("Arrow style: {}. Endpoints stay in place.", style.name);
    }
    fn update_saved_arrow(&mut self, name: &str) {
        let objects = self.selected_objects();
        if objects.len() != 1 || objects[0].kind != Kind::Arrow {
            self.status = "Select one arrow to update this saved style.".into();
            return;
        }
        let style = self.arrow_style(name.into());
        if let Some(store) = &mut self.presets.store {
            match store.save(style) {
                Ok(()) => {
                    self.presets.error.clear();
                    self.status = format!(
                        "Updated {name} from the selection. Existing arrows are unchanged."
                    );
                }
                Err(e) => self.presets.error = crate::guided::friendly(&e.to_string()),
            }
        }
    }
    pub(crate) fn arrow_styles(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Theme: ClipDesigner");
        });
        let families = [
            ("arrow", "Straight arrow", capture_core::styles::presets()),
            (
                "arrow_curved",
                "Curved arrow",
                capture_core::styles::curved_presets(),
            ),
        ];
        let hidden = self.quick_controls.preset_records().hidden;
        let saved: Vec<ArrowStyle> = self
            .presets
            .store
            .as_ref()
            .map(|s| s.entries().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !hidden.contains(&format!("arrow-saved:{}", s.name)))
            .collect();
        let current = self.arrow_style(String::new());
        let objects = self.selected_objects();
        let enabled = objects.iter().all(|a| a.kind == Kind::Arrow);
        let is_active = |style: &ArrowStyle| {
            current.color == style.color
                && current.width == style.width
                && current.shadow == style.shadow
                && current.line_style == style.line_style
                && objects.iter().all(|a| {
                    a.color == style.color
                        && a.width == style.width
                        && a.shadow == style.shadow
                        && a.line_style == style.line_style
                })
        };
        let mut chosen = None;
        let mut menu_choice: Option<(ArrowStyle, crate::preset_menu::Choice)> = None;
        let mut hide: Option<(String, String)> = None;
        let show_hidden_id = egui::Id::unique("arrow-gallery-show-hidden");
        let mut show_hidden = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(show_hidden_id))
            .unwrap_or(false);
        let mut remember = None;
        let mut save = None;
        egui::ScrollArea::vertical()
            .id_salt("arrow_presets")
            .max_height(214.0)
            .show(ui, |ui| {
                let saved_columns = crate::style_tile::columns(ui.available_width(), false);
                // Approved option B: "Save current" always ends the saved presets.
                {
                    ui.small("Your styles");
                    egui::Grid::new("saved_arrow_styles")
                        .spacing([5.0, 5.0])
                        .show(ui, |ui| {
                            for (i, style) in saved.iter().enumerate() {
                                let response = ui
                                    .add_enabled_ui(enabled, |ui| {
                                        arrow_tile(ui, style, &style.name, is_active(style))
                                    })
                                    .inner;
                                if response.clicked() {
                                    chosen = Some(style.clone());
                                }
                                // Approved removing presets: the shared menu and
                                // an × on hover.
                                let kit = self.kit_of("Arrow style", &style.name);
                                let menu = crate::preset_menu::Menu {
                                    apply: true,
                                    update: Some((
                                        "Update from selection",
                                        objects.len() == 1 && objects[0].kind == Kind::Arrow,
                                    )),
                                    rename: true,
                                    duplicate: true,
                                    hide: kit.is_some(),
                                    delete: true,
                                    kit: kit.as_deref(),
                                    name: &style.name,
                                    ..Default::default()
                                };
                                if let Some(choice) =
                                    crate::preset_menu::context_menu(&response, &menu)
                                {
                                    menu_choice = Some((style.clone(), choice));
                                }
                                if crate::preset_menu::hover_delete(
                                    ui,
                                    response.rect,
                                    response.id,
                                    &style.name,
                                ) {
                                    menu_choice =
                                        Some((style.clone(), crate::preset_menu::Choice::Delete));
                                }
                                if (i + 1) % saved_columns == 0 {
                                    ui.end_row();
                                }
                            }
                            save = Some(
                                ui.add_enabled_ui(
                                    objects.len() == 1 && enabled,
                                    crate::style_tile::save_tile,
                                )
                                .inner,
                            );
                        });
                    ui.separator();
                }
                ui.horizontal_top(|ui| {
                    for (key, title, styles) in &families {
                        if !show_hidden && hidden.contains(&format!("arrow:{key}")) {
                            continue;
                        }
                        let remembered = styles
                            .iter()
                            .find(|s| {
                                Some(s.name.as_str()) == self.quick_controls.style_choice(key)
                            })
                            .unwrap_or(&styles[0]);
                        ui.add_enabled_ui(enabled, |ui| {
                            crate::split_button::preset(ui, is_active(remembered), |ui| {
                                let main = crate::style_tile::split_tile(
                                    ui,
                                    &arrow_object_style(remembered),
                                    title,
                                    is_active(remembered),
                                );
                                if main.clicked() {
                                    chosen = Some(remembered.clone());
                                }
                                let hide_key = format!("arrow:{key}");
                                let menu = crate::preset_menu::Menu {
                                    apply: true,
                                    hide: !hidden.contains(&hide_key),
                                    name: title,
                                    ..Default::default()
                                };
                                match crate::preset_menu::context_menu(&main, &menu) {
                                    Some(crate::preset_menu::Choice::Apply) => {
                                        chosen = Some(remembered.clone())
                                    }
                                    Some(crate::preset_menu::Choice::Hide) => {
                                        hide = Some((hide_key, title.to_string()))
                                    }
                                    _ => {}
                                }
                                crate::split_button::dropdown_for(
                                    ui,
                                    &format!("{title} variations"),
                                    &main,
                                    |ui| {
                                        egui::Grid::new((key, "variations"))
                                            .spacing([5.0, 5.0])
                                            .show(ui, |ui| {
                                                for (i, style) in styles.iter().enumerate() {
                                                    if arrow_tile(
                                                        ui,
                                                        style,
                                                        &style.name,
                                                        is_active(style),
                                                    )
                                                    .clicked()
                                                    {
                                                        chosen = Some(style.clone());
                                                        remember = Some(*key);
                                                        ui.close();
                                                    }
                                                    if i % 3 == 2 {
                                                        ui.end_row();
                                                    }
                                                }
                                            });
                                    },
                                );
                            })
                        });
                    }
                });
            });
        let hidden_total = families
            .iter()
            .filter(|(key, _, _)| hidden.contains(&format!("arrow:{key}")))
            .count();
        if hidden_total > 0
            && crate::controls::toggle(
                ui,
                &mut show_hidden,
                &format!("Show hidden ({hidden_total})"),
            )
            .changed()
        {
            ui.ctx()
                .data_mut(|d| d.insert_temp(show_hidden_id, show_hidden));
        }
        if let Some((key, name)) = hide {
            self.hide_preset(key, &name);
        }
        if let Some(save) = save {
            self.preset_save_popup(ui, &save, Some(Kind::Arrow));
        }
        if let Some(style) = chosen {
            self.apply_arrow_style(&style);
            if let Some(key) = remember {
                self.quick_controls.remember_style(key, &style.name);
            }
        }
        if let Some((style, choice)) = menu_choice {
            use crate::preset_menu::Choice;
            match choice {
                Choice::Apply => self.apply_arrow_style(&style),
                Choice::Update => self.update_saved_arrow(&style.name),
                Choice::Rename(name) => {
                    if let Some(store) = &mut self.presets.store {
                        let mut renamed = style.clone();
                        renamed.name = name.clone();
                        match store.save(renamed).and_then(|()| store.remove(&style.name)) {
                            Ok(()) => self.status = format!("Renamed to {name}."),
                            Err(e) => self.fail(e),
                        }
                    }
                }
                Choice::Duplicate => {
                    if let Some(store) = &mut self.presets.store {
                        let mut copy = style.clone();
                        copy.name = crate::preset_menu::copy_name(&style.name, |n| {
                            store
                                .entries()
                                .iter()
                                .any(|s| s.name.eq_ignore_ascii_case(n))
                        });
                        let name = copy.name.clone();
                        match store.save(copy) {
                            Ok(()) => self.status = format!("Duplicated as {name}."),
                            Err(e) => self.fail(e),
                        }
                    }
                }
                Choice::Hide => {
                    self.hide_preset(format!("arrow-saved:{}", style.name), &style.name)
                }
                Choice::Delete => self.delete_presets(
                    &[crate::manage_presets::Item::Arrow(style.name.clone())],
                    &format!("\u{201c}{}\u{201d}", style.name),
                ),
                Choice::Favorite(_) => {}
            }
        }
        if !self.presets.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.presets.error);
        }
    }
}

fn arrow_tile(ui: &mut egui::Ui, arrow: &ArrowStyle, label: &str, active: bool) -> egui::Response {
    crate::style_tile::tile(ui, &arrow_object_style(arrow), label, active)
}
pub(crate) fn arrow_object_style(arrow: &ArrowStyle) -> capture_core::object_styles::ObjectStyle {
    let mut style = capture_core::object_styles::presets(Kind::Arrow).remove(0);
    style.name = arrow.name.clone();
    style.color = arrow.color;
    style.width = arrow.width;
    style.shadow = arrow.shadow;
    style.line_style = arrow.line_style;
    style
}

#[cfg(test)]
mod header_tests {
    use super::*;
    use crate::property_layout::tests::PaintedOutput;
    use egui::{Event, Modifiers, PointerButton};
    #[test]
    fn curved_arrow_family_applies_to_selection_and_next_arrow_with_one_undo() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let arrow = e.new_annotation(Kind::Arrow, [20., 20.], [150., 80.]);
        let id = arrow.id;
        e.execute(Command::Add {
            annotation: arrow.clone(),
        });
        e.select_only(Some(id));
        let frame = |e: &mut Editor, events| {
            let _output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(262., 500.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.arrow_styles(ui),
            ));
        };
        frame(&mut e, vec![]);
        frame(&mut e, vec![]);
        let tile = |label| {
            ctx.data(|d| {
                d.get_temp::<(egui::Rect, bool, bool)>(egui::Id::unique((
                    "style-tile",
                    "Arrow".to_owned(),
                    label,
                )))
            })
            .unwrap()
        };
        let curved = tile("Curved arrow").0.center();
        let straight = tile("Straight arrow").0;
        assert!(curved.x > straight.right());
        assert!((curved.y - straight.center().y).abs() < 0.1);
        assert!(!tile("Curved arrow").1);
        for pressed in [true, false] {
            frame(
                &mut e,
                vec![
                    Event::PointerMoved(curved),
                    Event::PointerButton {
                        pos: curved,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        frame(&mut e, vec![]);
        assert!(tile("Curved arrow").1);
        let edited = e.selected_annotation().unwrap();
        assert_eq!((edited.start, edited.end), (arrow.start, arrow.end));
        let style = edited.line_style.unwrap();
        assert_eq!(style.curve, capture_core::line_style::Curve::Circular);
        assert_eq!(style.bend, 0.25);
        assert_eq!(
            e.new_annotation(Kind::Arrow, [5., 5.], [50., 60.])
                .line_style,
            Some(style)
        );
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations[0], arrow);
    }
    fn text_rect(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<egui::Rect> {
        fn find(s: &egui::Shape, text: &str) -> Option<egui::Rect> {
            match s {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.galley.rect.translate(t.pos.to_vec2()))
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, text)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| find(&s.shape, text))
    }
    #[test]
    fn plus_popup_saves_and_immediately_lists_styles_without_document_edits() {
        let fixture: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        for kind in [Kind::Step, Kind::Rectangle, Kind::Arrow] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.0);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.session = capture_core::Session::new(fixture.clone()).unwrap();
            let id = fixture
                .annotations
                .iter()
                .find(|a| a.kind == kind)
                .unwrap()
                .id;
            e.select_only(Some(id));
            e.tool = Some(kind);
            let dir = tempfile::tempdir().unwrap();
            e.presets.templates =
                Some(capture_core::object_styles::Store::open(dir.path()).unwrap());
            e.presets.store = Some(Store::open(dir.path()).unwrap());
            let original = serde_json::to_vec(&e.session.document).unwrap();
            let deltas = std::cell::RefCell::new(egui::TexturesDelta::default());
            let frame = |e: &mut Editor, events| {
                let mut header = None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(262.0, 700.0),
                        )),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        header = Some(e.presets_header(ui, Some(kind)));
                        if kind == Kind::Arrow {
                            e.arrow_styles(ui);
                        } else {
                            e.object_templates(ui, kind);
                        }
                    },
                );
                deltas.borrow_mut().append(output.textures_delta.clone());
                (header.unwrap(), PaintedOutput(output))
            };
            let (plus, mut first) = frame(&mut e, vec![]);
            let (_, output) = frame(&mut e, vec![]);
            assert!(plus.rect.right() > 220.0 && plus.rect.bottom() <= 40.0);
            for old in [
                "Refresh",
                "Refresh styles",
                "Template name",
                "Save as template",
                "Style name",
            ] {
                assert!(text_rect(&output.shapes, old).is_none(), "{old}");
            }
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
            for pressed in [true, false] {
                frame(&mut e, click(plus.rect.center(), pressed));
            }
            frame(
                &mut e,
                vec![
                    Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::CTRL | Modifiers::COMMAND,
                    },
                    Event::Text("My saved style".into()),
                ],
            );
            let (_, output) = frame(&mut e, vec![]);
            let save = text_rect(&output.shapes, "Save Style").unwrap().center();
            if let Ok(root) = std::env::var("CLIPDESIGNER_STYLE_HEADER_DIR") {
                let mut textures = PaintedOutput(first.0.clone());
                textures.0.textures_delta.clear();
                textures.0.textures_delta = deltas.borrow().clone();
                crate::property_layout::tests::dump_meshes(
                    &ctx,
                    &root,
                    &format!("{kind:?}-popup"),
                    textures,
                    PaintedOutput(output.0.clone()),
                );
            }
            for pressed in [true, false] {
                frame(&mut e, click(save, pressed));
            }
            let (_, output) = frame(&mut e, vec![]);
            assert!(!egui::Popup::is_any_open(&ctx));
            assert!(text_rect(&output.shapes, "My saved style").is_some());
            if kind == Kind::Arrow {
                assert_eq!(
                    Store::open(dir.path()).unwrap().entries()[0].name,
                    "My saved style"
                );
            } else {
                assert_eq!(
                    capture_core::object_styles::Store::open(dir.path())
                        .unwrap()
                        .entries()[0]
                        .name,
                    "My saved style"
                );
            }
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
            assert!(!e.session.can_undo());
            first.0.textures_delta.clear();
            first.0.textures_delta = deltas.borrow().clone();
            deltas.borrow_mut().clear();
            if let Ok(root) = std::env::var("CLIPDESIGNER_STYLE_HEADER_DIR") {
                crate::property_layout::tests::dump_meshes(
                    &ctx,
                    &root,
                    &format!("{kind:?}"),
                    first,
                    output,
                );
            }
        }
    }
}
