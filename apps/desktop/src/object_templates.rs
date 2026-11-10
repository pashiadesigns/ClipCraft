use crate::tips::Tip as _;
use crate::{Editor, Kind, egui};
use capture_core::object_styles::ObjectStyle;

impl Editor {
    fn remembered_brush_preset<'a>(&'a self, preset: &'a ObjectStyle) -> &'a ObjectStyle {
        if matches!(preset.kind, Kind::Pen | Kind::Highlighter) {
            self.presets
                .drawing_styles
                .iter()
                .rev()
                .find(|s| s.same_variant(preset))
                .or_else(|| {
                    self.quick_controls
                        .drawing_variant(preset.kind, preset.highlighter_rect)
                })
                .unwrap_or(preset)
        } else {
            preset
        }
    }

    pub(super) fn drawing_rectangle_highlighter(&self) -> bool {
        self.presets
            .drawing_styles
            .iter()
            .rev()
            .find(|s| s.kind == Kind::Highlighter)
            .or_else(|| self.quick_controls.drawing_style(Kind::Highlighter))
            .is_some_and(|s| s.highlighter_rect)
    }
    pub(super) fn remember_annotation_style(&mut self, annotation: &crate::Annotation) {
        if annotation.focus.is_some() {
            self.shell.focus.remember(annotation);
            return;
        }
        if let Ok(style) = ObjectStyle::from_annotation("Last used".into(), annotation) {
            self.presets
                .drawing_styles
                .retain(|s| !s.same_variant(&style));
            self.presets.drawing_styles.push(style.clone());
            self.quick_controls.remember_drawing_style(style);
            if self.tool == Some(annotation.kind) {
                self.restore_drawing_defaults(self.tool);
            }
        }
    }
    pub(super) fn restore_drawing_defaults(&mut self, kind: Option<Kind>) {
        if let Some(style) = kind.and_then(|k| {
            self.presets
                .drawing_styles
                .iter()
                .rev()
                .find(|s| s.kind == k && s.focus.is_none())
                .or_else(|| {
                    self.quick_controls
                        .drawing_style(k)
                        .filter(|s| s.focus.is_none())
                })
        }) {
            self.color = style.color[..3].try_into().unwrap();
            self.width = style.width;
            self.presets.alpha = style.color[3];
        } else if let Some(kind) = kind {
            // Each tool starts at its own first preset's width (3 for lines and
            // arrows), not whatever the last tool used.
            self.width = capture_core::object_styles::presets(kind)
                .first()
                .map_or(3, |s| s.width);
        }
    }
    pub(super) fn apply_object_template(&mut self, style: &ObjectStyle, custom: bool) {
        if let Err(error) = style.validate() {
            self.status = error.to_string();
            return;
        }
        let objects = self.selected_objects();
        if !objects.iter().all(|a| style.accepts(a.kind)) {
            self.status = "Select compatible objects to apply this style.".into();
            return;
        }
        if let Some(draft) = &mut self.text_draft {
            match style.apply_compatible(&draft.preview) {
                Ok(preview) => {
                    draft.preview = preview;
                    self.needs_render = true;
                }
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            }
        } else if !objects.is_empty() {
            match style.compatible_command(&objects) {
                Ok(command) => self.execute(command),
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            }
        }
        self.presets
            .drawing_styles
            .retain(|s| !s.same_variant(style));
        self.presets.drawing_styles.push(style.clone());
        self.quick_controls.remember_drawing_style(style.clone());
        self.color = style.color[..3].try_into().unwrap();
        self.width = style.width;
        self.presets.alpha = style.color[3];
        if self.tool.is_some() {
            self.tool = Some(style.kind);
        }
        self.presets.template_chosen = Some((style.kind, style.name.clone(), custom));
        self.status = format!(
            "{} applied. New objects use this style; Undo restores the selection.",
            style.name
        );
    }
    pub(super) fn styled_new_annotation(&self, annotation: crate::Annotation) -> crate::Annotation {
        let Some(style) = self
            .presets
            .drawing_styles
            .iter()
            .rev()
            .find(|s| s.kind == annotation.kind && s.focus.is_none())
            .or_else(|| {
                self.quick_controls
                    .drawing_style(annotation.kind)
                    .filter(|s| s.focus.is_none())
            })
        else {
            return annotation;
        };
        match style.apply(&annotation) {
            Ok(mut styled) => {
                styled.color = [
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    self.presets.alpha,
                ];
                styled.width = self.width;
                styled
            }
            Err(_) => annotation,
        }
    }
    fn update_saved_template(&mut self, kind: Kind, name: &str) {
        let objects = self.selected_objects();
        let selected = self.selected_annotation();
        let Some(source) = selected.filter(|a| objects.len() == 1 && a.kind == kind) else {
            self.status = "Select one object of the saved style's type to update it.".into();
            return;
        };
        let Some(store) = &mut self.presets.templates else {
            return;
        };
        match ObjectStyle::from_annotation(name.into(), &source).and_then(|style| store.save(style))
        {
            Ok(()) => {
                self.presets.template_error.clear();
                self.presets.template_chosen = Some((kind, name.into(), true));
                self.status =
                    format!("Updated {name} from the selection. Existing objects are unchanged.");
            }
            Err(error) => self.presets.template_error = error.to_string(),
        }
    }
    pub(super) fn save_selected_template(&mut self) -> bool {
        let objects = self.selected_objects();
        if objects.len() != 1 {
            self.status = "Select one source object to save its formatting.".into();
            return false;
        }
        let selected = self
            .selected_annotation()
            .unwrap_or_else(|| objects[0].clone());
        let a = &selected;
        let Some(store) = &mut self.presets.templates else {
            return false;
        };
        let name = if self.presets.template_name.trim().is_empty() {
            let mut n = 1;
            loop {
                let name = format!("{:?} {n}", a.kind);
                if !store
                    .entries()
                    .iter()
                    .any(|s| s.kind == a.kind && s.name.eq_ignore_ascii_case(&name))
                {
                    break name;
                }
                n += 1;
            }
        } else {
            self.presets.template_name.trim().to_owned()
        };
        match ObjectStyle::from_annotation(name.clone(), a).and_then(|s| store.save(s)) {
            Ok(()) => {
                self.presets.template_error.clear();
                self.presets.template_chosen = Some((a.kind, name.clone(), true));
                self.status =
                    format!("Saved {name}. Select matching objects and click its tile to apply.");
                true
            }
            Err(error) => {
                self.presets.template_error = error.to_string();
                false
            }
        }
    }
    pub(super) fn object_templates(&mut self, ui: &mut egui::Ui, kind: Kind) {
        let objects = self.selected_objects();
        let display_objects = self
            .selected_annotation()
            .filter(|_| objects.len() == 1)
            .map(|a| vec![a])
            .unwrap_or_else(|| objects.clone());
        let saved: Vec<_> = self
            .presets
            .templates
            .as_ref()
            .map(|store| {
                store
                    .entries()
                    .iter()
                    .filter(|s| s.focus.is_none() && s.accepts(kind))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let groups = capture_core::object_styles::groups(kind);
        let is_active = |style: &ObjectStyle| {
            style.matches(&display_objects)
                || (objects.is_empty()
                    && self.tool == Some(style.kind)
                    && style.matches(&[self.new_annotation(
                        style.kind,
                        [20.0, 20.0],
                        [80.0, 60.0],
                    )]))
        };
        let mut apply = None;
        let mut remove = None;
        let mut update = None;
        let mut save = None;
        egui::ScrollArea::vertical()
            .id_salt("object_templates")
            .max_height(214.0)
            .show(ui, |ui| {
                let saved_columns = crate::style_tile::columns(ui.available_width(), false);
                let columns = crate::style_tile::columns(ui.available_width(), true);
                // Approved option B: "Save current" always ends the saved presets.
                {
                    ui.small("Your styles");
                    egui::Grid::new("saved_templates")
                        .spacing([5.0, 5.0])
                        .show(ui, |ui| {
                            for (index, style) in saved.iter().enumerate() {
                                let enabled = objects.iter().all(|a| style.accepts(a.kind));
                                let active = is_active(style);
                                let response = ui
                                    .add_enabled_ui(enabled, |ui| {
                                        crate::style_tile::tile(ui, style, &style.name, active)
                                    })
                                    .inner;
                                if response.clicked() {
                                    apply = Some((style.clone(), true, None));
                                }
                                response.context_menu(|ui| {
                                    if ui
                                        .add_enabled_ui(
                                            objects.len() == 1 && objects[0].kind == style.kind,
                                            |ui| {
                                                crate::controls::menu_action(
                                                    ui,
                                                    "Update from selection",
                                                )
                                            },
                                        )
                                        .inner
                                        .clicked()
                                    {
                                        update = Some((style.kind, style.name.clone()));
                                        ui.close();
                                    }
                                    if crate::controls::menu_action(ui, "Delete style").clicked() {
                                        remove = Some((style.kind, style.name.clone()));
                                        ui.close();
                                    }
                                });
                                if (index + 1) % saved_columns == 0 {
                                    ui.end_row();
                                }
                            }
                            save = Some(
                                ui.add_enabled_ui(objects.len() == 1, |ui| {
                                    crate::style_tile::save_tile(ui)
                                })
                                .inner,
                            );
                        });
                    ui.separator();
                }
                for row in groups.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        for group in row {
                            let last = self.quick_controls.style_choice(group.id);
                            let remembered = group
                                .styles
                                .iter()
                                .find(|s| Some(s.name.as_str()) == last)
                                .unwrap_or(&group.styles[0]);
                            let remembered = self.remembered_brush_preset(remembered);
                            let enabled = objects.iter().all(|a| remembered.accepts(a.kind));
                            ui.add_enabled_ui(enabled, |ui| {
                                crate::split_button::preset(ui, is_active(remembered), |ui| {
                                    let response = crate::style_tile::split_tile(
                                        ui,
                                        remembered,
                                        group.label,
                                        is_active(remembered),
                                    );
                                    response.clone().tip(format!(
                                        "{}: {}. Click to repeat this style.",
                                        group.label, remembered.name
                                    ));
                                    if response.clicked() {
                                        apply = Some((remembered.clone(), false, Some(group.id)));
                                    }
                                    crate::split_button::dropdown_for(
                                        ui,
                                        &format!("{} variations", group.label),
                                        &response,
                                        |ui| {
                                            ui.strong(group.label);
                                            egui::Grid::new(group.id).spacing([5.0, 5.0]).show(
                                                ui,
                                                |ui| {
                                                    for (n, style) in
                                                        group.styles.iter().enumerate()
                                                    {
                                                        if crate::style_tile::tile(
                                                            ui,
                                                            style,
                                                            &style.name,
                                                            is_active(style),
                                                        )
                                                        .clicked()
                                                        {
                                                            apply = Some((
                                                                style.clone(),
                                                                false,
                                                                Some(group.id),
                                                            ));
                                                            ui.close();
                                                        }
                                                        if n % 3 == 2 {
                                                            ui.end_row();
                                                        }
                                                    }
                                                },
                                            );
                                        },
                                    );
                                });
                            });
                        }
                    });
                }
            });
        if let Some(save) = save {
            self.preset_save_popup(ui, &save, Some(kind));
        }
        if let Some((style, custom, category)) = apply {
            self.apply_object_template(&style, custom);
            if let Some(category) = category {
                self.quick_controls.remember_style(category, &style.name);
            }
        }
        if let Some((kind, name)) = update {
            self.update_saved_template(kind, &name);
        }
        if let Some((kind, name)) = remove
            && let Some(store) = &mut self.presets.templates
        {
            match store.remove(kind, &name) {
                Ok(()) => {
                    self.presets.template_error.clear();
                    self.status = format!("Deleted {name}. Existing objects are unchanged.");
                }
                Err(e) => self.presets.template_error = e.to_string(),
            }
        }
        if !self.presets.template_error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.presets.template_error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_style_update_delete_and_new_defaults_preserve_document_history() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let a = capture_core::steps::marker(
            1,
            [80.0, 80.0],
            44,
            [30, 60, 90, 255],
            (1, false),
            [640, 400],
        )
        .unwrap();
        e.session.document.annotations = vec![a.clone()];
        e.select_only(Some(1));
        e.tool = Some(Kind::Step);
        let dir = tempfile::tempdir().unwrap();
        e.presets.templates = Some(capture_core::object_styles::Store::open(dir.path()).unwrap());
        e.presets.template_name = "My marker".into();
        e.save_selected_template();
        let style = capture_core::object_styles::groups(Kind::Step)
            .remove(0)
            .styles
            .remove(2);
        e.apply_object_template(&style, false);
        let expected = e.session.document.annotations.clone();
        e.update_saved_template(Kind::Step, "My marker");
        assert_eq!(e.session.document.annotations, expected);
        let loaded = capture_core::object_styles::Store::open(dir.path()).unwrap();
        assert_eq!(loaded.entries().len(), 1);
        assert_eq!(loaded.entries()[0].name, "My marker");
        assert!(loaded.entries()[0].matches(&expected));
        e.select_only(None);
        e.apply_object_template(&loaded.entries()[0], true);
        let mut next = a.clone();
        next.id = 2;
        next.text.as_mut().unwrap().content = "2".into();
        let drawn = e.styled_new_annotation(next.clone());
        assert_eq!(drawn.color, style.color);
        assert_eq!(drawn.text.as_ref().unwrap().content, "2");
        e.presets
            .templates
            .as_mut()
            .unwrap()
            .remove(Kind::Step, "My marker")
            .unwrap();
        assert!(
            capture_core::object_styles::Store::open(dir.path())
                .unwrap()
                .entries()
                .is_empty()
        );
        assert_eq!(e.session.document.annotations, expected);
        e.execute(crate::Command::Undo);
        assert_eq!(e.session.document.annotations, vec![a]);
        assert!(!e.session.can_undo());
        e.color = [1, 2, 3];
        e.width = 8;
        let drawn = e.styled_new_annotation(next);
        assert_eq!(drawn.color[..3], [1, 2, 3]);
        assert_eq!(drawn.width, 8);
    }
    #[test]
    fn save_current_tile_ends_saved_presets_and_opens_the_save_popup() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session.document.annotations.push(
            capture_core::steps::marker(
                1,
                [100.0, 100.0],
                44,
                [120, 20, 40, 255],
                (1, false),
                [640, 400],
            )
            .unwrap(),
        );
        e.select_only(Some(1));
        let dir = tempfile::tempdir().unwrap();
        e.presets.templates = Some(capture_core::object_styles::Store::open(dir.path()).unwrap());
        let frame = |e: &mut Editor, events: Vec<egui::Event>| {
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(340., 600.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.object_templates(ui, Kind::Step),
            );
            crate::property_layout::tests::PaintedOutput(out)
        };
        frame(&mut e, vec![]);
        frame(&mut e, vec![]);
        let tile = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("preset-save-tile")))
            .expect("Save current tile drawn with no saved presets yet");
        for pressed in [true, false] {
            frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(tile.center()),
                    egui::Event::PointerButton {
                        pos: tile.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let out = frame(&mut e, vec![]);
        assert!(crate::color_widgets::tests::text_rect(&out.shapes, "Save Style").is_some());
    }
    #[test]
    fn template_clicks_batch_apply_preserve_labels_and_save_reopen_without_document_edits() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for id in 1..=3 {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    [id as f32 * 100.0, 100.0],
                    44,
                    [120, 20, 40, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        e.tool = Some(Kind::Step);
        e.select_all_matching();
        let original = e.session.document.annotations.clone();
        for style in &capture_core::object_styles::presets(Kind::Step)[..3] {
            e.apply_object_template(style, false);
            assert!(style.matches(&e.selected_objects()));
            for (a, b) in original.iter().zip(&e.session.document.annotations) {
                assert_eq!(a.start, b.start);
                assert_eq!(
                    a.text.as_ref().unwrap().content,
                    b.text.as_ref().unwrap().content
                );
            }
        }
        let expected = e.session.document.annotations.clone();
        e.select_only(Some(1));
        let dir = tempfile::tempdir().unwrap();
        e.presets.templates = Some(capture_core::object_styles::Store::open(dir.path()).unwrap());
        e.save_selected_template();
        assert_eq!(e.session.document.annotations, expected);
        let store = capture_core::object_styles::Store::open(dir.path()).unwrap();
        assert_eq!(store.entries().len(), 1);
        assert!(store.entries()[0].matches(&[expected[0].clone()]));
        e.execute(crate::Command::Undo);
        assert_eq!(
            e.session.document.annotations[0].color,
            [135, 196, 105, 255]
        );
        e.select_all_matching();
        e.apply_object_template(&store.entries()[0], true);
        assert_eq!(e.session.document.annotations, expected);
    }
}

#[cfg(test)]
#[test]
fn penlighter_primary_presets_keep_separate_width_and_opacity() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.tool = Some(Kind::Pen);
    for (kind, width, alpha) in [(Kind::Pen, 7, 190), (Kind::Highlighter, 25, 65)] {
        e.tool = Some(kind);
        e.width = width;
        e.presets.alpha = alpha;
        let example = e.new_annotation(kind, [30., 30.], [100., 60.]);
        e.remember_annotation_style(&example);
    }
    for kind in [Kind::Pen, Kind::Highlighter, Kind::Pen, Kind::Highlighter] {
        let groups = capture_core::object_styles::groups(kind);
        let preset = groups
            .iter()
            .flat_map(|g| &g.styles)
            .find(|s| s.kind == kind && !s.highlighter_rect)
            .unwrap();
        let restored = e.remembered_brush_preset(preset).clone();
        e.apply_object_template(&restored, false);
        let expected = if kind == Kind::Pen {
            (7, 190)
        } else {
            (25, 65)
        };
        assert_eq!((e.width, e.presets.alpha), expected);
        assert_eq!(e.tool, Some(kind));
    }
    assert!(e.session.document.annotations.is_empty());
}
