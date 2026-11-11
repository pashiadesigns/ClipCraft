use crate::egui;
use crate::tips::Tip as _;
use capture_core::reusable_presets::{Payload, Store};
#[derive(Default)]
pub(crate) struct State {
    store: Option<Store>,
    #[cfg(test)]
    test_root: Option<tempfile::TempDir>,
    initialized: bool,
    error: String,
    query: String,
    favorites: bool,
    open: bool,
    current: Option<Payload>,
    edit: Option<(Option<u64>, String, Payload)>,
    focus: bool,
    delete: Option<u64>,
    group_only: bool,
    placed_group: Option<capture_core::group_presets::Composition>,
    group_thumbs: std::collections::BTreeMap<u64, egui::TextureHandle>,
    /// The save dialog renames an entry instead of saving settings.
    rename: bool,
    /// The save dialog replaces an existing entry rather than adding one.
    replace: bool,
    /// Closing the save dialog returns to the library it was opened from.
    return_to_library: bool,
    filter: Filter,
    selected: Option<u64>,
}

/// The Blocks dialog's filters (approved `artifacts/beta5-proposal`, tab 2).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    All,
    Favorites,
    Recent,
}

/// Width of the save dialog (approved compact dialog, tab 2).
const SAVE_WIDTH: f32 = 420.;

impl State {
    /// Opens Blocks. `current` is the selection as a block, if any, for the
    /// cards' "Update from selection".
    pub(crate) fn open_groups(&mut self, current: Option<Payload>) {
        self.init();
        self.group_only = true;
        self.current = current;
        self.query.clear();
        self.selected = None;
        self.open = true;
        self.focus = true;
    }
    /// The payload waiting in the save dialog, for tests.
    #[cfg(test)]
    pub(crate) fn pending_save(&self) -> Option<&Payload> {
        self.edit.as_ref().map(|(_, _, p)| p)
    }
    /// Saved Blocks and other presets, for kits.
    pub(crate) fn store_mut(&mut self) -> Option<&mut Store> {
        self.init();
        self.group_thumbs.clear();
        self.store.as_mut()
    }
    pub(crate) fn take_group(&mut self) -> Option<capture_core::group_presets::Composition> {
        self.placed_group.take()
    }
    pub(crate) fn save_payload(&mut self, payload: Payload) {
        self.init();
        self.current = Some(payload.clone());
        self.edit = Some((None, String::new(), payload));
        self.rename = false;
        self.replace = false;
        self.return_to_library = false;
        self.focus = true;
        self.open = true;
        self.error.clear();
    }
    /// A block's preview, rendered once at `THUMB` and drawn fitted into cards.
    fn group_texture(
        &mut self,
        ctx: &egui::Context,
        id: u64,
        c: &capture_core::group_presets::Composition,
    ) -> Option<egui::TextureHandle> {
        const THUMB: [u32; 2] = [320, 200];
        if let std::collections::btree_map::Entry::Vacant(entry) = self.group_thumbs.entry(id) {
            let mut doc = capture_core::Document::blank(
                c.objects.source_canvas[0],
                c.objects.source_canvas[1],
            )
            .ok()?;
            doc.annotations = c.objects.objects.clone();
            let mut lo = [f32::INFINITY; 2];
            let mut hi = [f32::NEG_INFINITY; 2];
            for a in &doc.annotations {
                let (l, h) = a.bounds();
                for i in 0..2 {
                    lo[i] = lo[i].min(l[i]);
                    hi[i] = hi[i].max(h[i]);
                }
            }
            let pad = 18.;
            let scale = ((f64::from(THUMB[0]) - 2. * pad) / f64::from((hi[0] - lo[0]).max(1.)))
                .min((f64::from(THUMB[1]) - 2. * pad) / f64::from((hi[1] - lo[1]).max(1.)));
            let size = [
                f64::from(hi[0] - lo[0]) * scale,
                f64::from(hi[1] - lo[1]) * scale,
            ];
            let origin = [
                f64::from(lo[0]) - (f64::from(THUMB[0]) - size[0]) / 2. / scale,
                f64::from(lo[1]) - (f64::from(THUMB[1]) - size[1]) / 2. / scale,
            ];
            if let Ok(list) = doc.display_list()
                && let Ok(p) = list.render_viewport(capture_core::display::Viewport {
                    size: THUMB,
                    origin,
                    scale,
                })
            {
                entry.insert(ctx.load_texture(
                    format!("group-preset-{id}"),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [THUMB[0] as usize, THUMB[1] as usize],
                        p.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        self.group_thumbs.get(&id).cloned()
    }

    fn init(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        #[cfg(not(test))]
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
        #[cfg(test)]
        let root = {
            self.test_root = Some(tempfile::tempdir().expect("isolated preset storage"));
            Some(self.test_root.as_ref().unwrap().path().to_path_buf())
        };
        match root.map(|p| Store::open(&p)) {
            Some(Ok(s)) => self.store = Some(s),
            Some(Err(e)) => self.error = e.to_string(),
            None => self.error = "Local preset storage is unavailable.".into(),
        }
    }
    pub(crate) fn controls(&mut self, ui: &mut egui::Ui, current: Payload) -> Option<Payload> {
        self.init();
        let mut chosen = None;
        ui.push_id(current.category(), |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("saved-creative-presets")
                    .width((ui.available_width() - 72.).max(100.))
                    .selected_text(
                        self.store
                            .as_ref()
                            .and_then(|s| s.entries().iter().find(|e| e.payload == current))
                            .map_or("Saved presets", |e| e.name.as_str()),
                    )
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show_ui(ui, |ui| {
                        ui.add(
                            crate::unicode_input::TextEdit::singleline(&mut self.query)
                                .hint_text("Search presets")
                                .desired_width(190.),
                        );
                        let mut entries = self
                            .store
                            .as_ref()
                            .map(|s| s.entries().to_vec())
                            .unwrap_or_default();
                        entries.sort_by_key(|e| (!e.favorite, e.name.to_lowercase()));
                        egui::ScrollArea::vertical()
                            .max_height(240.)
                            .show(ui, |ui| {
                                let mut count = 0;
                                for e in entries.into_iter().filter(|e| {
                                    e.payload.compatible(&current)
                                        && e.name
                                            .to_lowercase()
                                            .contains(&self.query.to_lowercase())
                                }) {
                                    count += 1;
                                    if ui
                                        .button(if e.favorite {
                                            format!("{} · favorite", e.name)
                                        } else {
                                            e.name
                                        })
                                        .clicked()
                                    {
                                        chosen = Some(e.payload);
                                        ui.close();
                                    }
                                }
                                if count == 0 {
                                    ui.weak("No matching saved presets");
                                }
                            });
                        crate::panel_ui::divider(ui);
                        if ui.button("Manage / share presets…").clicked() {
                            self.group_only = false;
                            self.current = Some(current.clone());
                            self.open = true;
                            self.focus = true;
                            ui.close();
                        }
                    });
                if ui
                    .button("+ Save")
                    .tip("Save the current settings as a reusable preset")
                    .clicked()
                {
                    self.group_only = false;
                    self.current = Some(current.clone());
                    self.edit = Some((None, String::new(), current.clone()));
                    self.rename = false;
                    self.replace = false;
                    self.return_to_library = false;
                    self.focus = true;
                    self.open = true;
                    self.error.clear();
                }
            });
        });
        chosen
    }
    pub(crate) fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        if self.edit.is_some() {
            self.show_save(ctx);
        } else if self.group_only {
            self.show_blocks(ctx);
        } else {
            self.show_library(ctx);
        }
    }

    /// Save as block / Save preset / Rename: a compact dialog sized to its
    /// content, Save and Cancel pinned to the footer (approved tab 2).
    fn show_save(&mut self, ctx: &egui::Context) {
        let mut save = false;
        let mut cancel = false;
        let rename = self.rename;
        let compatible = self
            .edit
            .as_ref()
            .map(|(_, _, payload)| {
                self.store
                    .as_ref()
                    .map(|s| {
                        s.entries()
                            .iter()
                            .filter(|e| e.payload.compatible(payload))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let Some((id, label, payload)) = &mut self.edit else {
            return;
        };
        let block = matches!(payload, Payload::Group(_));
        let noun = if block { "block" } else { "preset" };
        let title = if rename {
            "Rename".to_owned()
        } else if block {
            "Save as block".to_owned()
        } else {
            "Save preset".to_owned()
        };
        let response =
            crate::controls::Modal::new(egui::Id::unique("save-preset-dialog")).show(ctx, |ui| {
                ui.set_width(SAVE_WIDTH);
                crate::controls::dialog_title(ui, &title);
                crate::controls::dialog_section(ui, "Name", None);
                let r = ui.add(
                    crate::unicode_input::TextEdit::singleline(label)
                        .char_limit(80)
                        .desired_width(f32::INFINITY),
                );
                if self.focus {
                    r.request_focus();
                    self.focus = false;
                }
                if !rename {
                    ui.add_space(8.);
                    let new = format!("Save as a new {noun}");
                    let old = format!("Replace an existing {noun}");
                    if choice_row(
                        ui,
                        !self.replace,
                        &new,
                        &format!("Adds it to your {}s.", noun),
                    )
                    .clicked()
                    {
                        self.replace = false;
                        *id = None;
                    }
                    if choice_row(
                        ui,
                        self.replace,
                        &old,
                        if block {
                            "Updates the one you pick with the selected objects."
                        } else {
                            "Updates the one you pick with the current settings."
                        },
                    )
                    .clicked()
                    {
                        self.replace = true;
                    }
                    ui.add_enabled_ui(self.replace && !compatible.is_empty(), |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(128.)
                            .id_salt("save-compatible-presets")
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                for e in &compatible {
                                    if ui.selectable_label(*id == Some(e.id), &e.name).clicked() {
                                        *id = Some(e.id);
                                        *label = e.name.clone();
                                    }
                                }
                            });
                    });
                    if compatible.is_empty() && self.replace {
                        ui.weak(format!("There is no compatible {noun} to replace yet."));
                    }
                }
                if !self.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                }
                ui.add_space(12.);
                crate::panel_ui::divider(ui);
                ui.add_space(4.);
                // A row of its own: a bare right-to-left layout would take the
                // dialog's whole remaining height.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready =
                            !label.trim().is_empty() && (rename || !self.replace || id.is_some());
                        if ui
                            .add_enabled(ready, crate::controls::dialog_button("Save", true))
                            .clicked()
                        {
                            save = true;
                        }
                        if ui
                            .add(crate::controls::dialog_button("Cancel", false))
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
            });
        if save && let Some((id, name, payload)) = self.edit.clone() {
            let result = self
                .store
                .as_mut()
                .ok_or_else(|| "Local preset storage is unavailable.".to_string())
                .and_then(|s| match id {
                    Some(id) if self.rename => s.rename(id, &name).map_err(|e| e.to_string()),
                    Some(id) => s.update(id, &name, payload).map_err(|e| e.to_string()),
                    None => s
                        .save(&name, payload)
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                });
            match result {
                Ok(()) => {
                    self.group_thumbs.clear();
                    self.query.clear();
                    self.error.clear();
                    self.finish_save();
                }
                Err(e) => self.error = e,
            }
        }
        if cancel || response.should_close() {
            self.error.clear();
            self.finish_save();
        }
    }

    fn finish_save(&mut self) {
        self.edit = None;
        self.rename = false;
        self.replace = false;
        if !std::mem::take(&mut self.return_to_library) {
            self.open = false;
        }
    }

    /// Blocks: a card grid with a large preview, the favorite star on the
    /// preview, Insert on the hovered or selected card and in the footer, and a
    /// ⋯ menu per card (approved tab 2).
    fn show_blocks(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut insert = None;
        let entries: Vec<_> = self
            .store
            .as_ref()
            .map(|s| s.entries().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|e| matches!(e.payload, Payload::Group(_)))
            .collect();
        let response = crate::controls::Modal::new(egui::Id::unique("blocks-dialog")).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 80.).clamp(320., 820.);
            ui.set_width(width);
            crate::controls::dialog_title(ui, "Blocks");
            ui.horizontal(|ui| {
                let r = ui.add(
                    crate::unicode_input::TextEdit::singleline(&mut self.query)
                        .hint_text("Search blocks")
                        .desired_width((width - 300.).max(140.)),
                );
                if self.focus {
                    r.request_focus();
                    self.focus = false;
                }
                for (filter, label) in [
                    (Filter::All, "All"),
                    (Filter::Favorites, "Favorites"),
                    (Filter::Recent, "Recently used"),
                ] {
                    if filter_chip(ui, label, self.filter == filter).clicked() {
                        self.filter = filter;
                    }
                }
            });
            ui.add_space(8.);
            let query = self.query.to_lowercase();
            let mut shown: Vec<_> = entries
                .iter()
                .filter(|e| match self.filter {
                    Filter::All => true,
                    Filter::Favorites => e.favorite,
                    Filter::Recent => e.used > 0,
                })
                .filter(|e| e.name.to_lowercase().contains(&query))
                .cloned()
                .collect();
            if self.filter == Filter::Recent {
                shown.sort_by_key(|e| std::cmp::Reverse(e.used));
            }
            const GAP: f32 = 12.;
            let columns = ((width + GAP) / (180. + GAP)).floor().max(1.) as usize;
            let card_width = (width - GAP * (columns as f32 - 1.)) / columns as f32;
            egui::ScrollArea::vertical()
                .id_salt("blocks-grid")
                .max_height((ctx.content_rect().height() - 260.).clamp(200., 460.))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
                    for row in shown.chunks(columns) {
                        ui.horizontal(|ui| {
                            for e in row {
                                if let Some(action) = self.block_card(ui, e, card_width) {
                                    match action {
                                        CardAction::Insert => insert = Some(e.id),
                                        CardAction::Select => self.selected = Some(e.id),
                                    }
                                }
                            }
                        });
                    }
                    if shown.is_empty() {
                        ui.weak(if entries.is_empty() {
                            "Select objects and choose Save as block to start your Blocks."
                        } else {
                            "No blocks match."
                        });
                    }
                });
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
            }
            ui.add_space(10.);
            crate::panel_ui::divider(ui);
            ui.add_space(4.);
            ui.horizontal(|ui| {
                let selected = self
                    .selected
                    .and_then(|id| entries.iter().find(|e| e.id == id));
                ui.weak(match selected {
                    Some(e) => format!("Selected: {}. Double-click a block or press Insert.", e.name),
                    None => "Double-click a block or press Insert to add it at the center of the view.".into(),
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(selected.is_some(), crate::controls::dialog_button("Insert", true))
                        .clicked()
                    {
                        insert = self.selected;
                    }
                    if ui.add(crate::controls::dialog_button("Close", false)).clicked() {
                        close = true;
                    }
                });
            });
        });
        if let Some(id) = insert
            && let Some(Payload::Group(c)) = entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.payload.clone())
        {
            self.placed_group = Some(c);
            self.result(|s| s.mark_used(id));
            close = true;
        }
        if close || response.should_close() {
            self.open = false;
            self.delete = None;
        }
    }

    fn block_card(
        &mut self,
        ui: &mut egui::Ui,
        e: &capture_core::reusable_presets::Entry,
        width: f32,
    ) -> Option<CardAction> {
        use crate::tips::Tip as _;
        const META: f32 = 46.;
        let preview_height = (width * 10. / 16.).round();
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(width, preview_height + META),
            egui::Sense::click(),
        );
        let selected = self.selected == Some(e.id);
        let hovered = ui.rect_contains_pointer(rect);
        let painter = ui.painter_at(rect.expand(2.));
        painter.rect_filled(rect, 10., egui::Color32::from_rgb(58, 66, 70));
        let preview = egui::Rect::from_min_size(rect.min, egui::vec2(width, preview_height));
        painter.rect_filled(
            preview,
            egui::CornerRadius {
                nw: 10,
                ne: 10,
                sw: 0,
                se: 0,
            },
            egui::Color32::from_rgb(244, 246, 248),
        );
        if let Payload::Group(c) = &e.payload
            && let Some(texture) = self.group_texture(ui.ctx(), e.id, c)
        {
            let fit = (preview.width() / 320.).min(preview.height() / 200.);
            let image =
                egui::Rect::from_center_size(preview.center(), egui::vec2(320., 200.) * fit);
            painter.image(
                texture.id(),
                image,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
        if selected {
            painter.rect_stroke(
                rect,
                10.,
                egui::Stroke::new(2., crate::shell::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        let mut action = None;
        // Favorite star on the preview's corner.
        let star = egui::Rect::from_center_size(
            preview.right_top() + egui::vec2(-18., 18.),
            egui::vec2(26., 26.),
        );
        let star_response = ui
            .interact(
                star,
                ui.scope_id().with(("block-star", e.id)),
                egui::Sense::click(),
            )
            .tip(if e.favorite {
                "Remove from favorites"
            } else {
                "Add to favorites"
            });
        // A favorite sits on an amber badge so it reads at a glance.
        painter.circle_filled(
            star.center(),
            13.,
            if e.favorite {
                egui::Color32::from_rgb(227, 162, 59)
            } else {
                egui::Color32::from_black_alpha(if star_response.hovered() { 170 } else { 120 })
            },
        );
        crate::icons::draw_scaled(&painter, star, "Favorites", egui::Color32::WHITE, 0.5);
        if star_response.clicked() {
            let id = e.id;
            let value = !e.favorite;
            self.result(|s| s.favorite(id, value));
        }
        // Insert on the hovered or selected card, with room below it.
        if hovered || selected {
            let button = egui::Rect::from_min_max(
                egui::pos2(preview.left() + 10., preview.bottom() - 10. - 28.),
                egui::pos2(preview.right() - 10., preview.bottom() - 10.),
            );
            let insert = ui.put(
                button,
                egui::Button::new(egui::RichText::new("Insert").strong())
                    .fill(crate::shell::ACCENT),
            );
            if insert.clicked() {
                action = Some(CardAction::Insert);
            }
        }
        // Name, category and the ⋯ menu.
        let meta = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 10., preview.bottom() + 4.),
            egui::pos2(rect.right() - 6., rect.bottom() - 4.),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(meta)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.vertical(|ui| {
                    ui.set_width(meta.width() - 34.);
                    ui.add(egui::Label::new(crate::panel_ui::title(&e.name, 13.)).truncate());
                    ui.small(if e.used > 0 {
                        "Block · used recently"
                    } else {
                        "Block"
                    });
                });
                ui.menu_button("…", |ui| {
                    if crate::controls::menu_action(ui, "Rename").clicked() {
                        self.edit = Some((Some(e.id), e.name.clone(), e.payload.clone()));
                        self.rename = true;
                        self.return_to_library = true;
                        self.focus = true;
                        self.error.clear();
                        ui.close();
                    }
                    if crate::controls::menu_action(ui, "Duplicate").clicked() {
                        let id = e.id;
                        self.result(|s| s.duplicate(id).map(|_| ()));
                        ui.close();
                    }
                    if let Some(current) = self.current.clone()
                        && current.compatible(&e.payload)
                        && crate::controls::menu_action(ui, "Update from selection").clicked()
                    {
                        let id = e.id;
                        self.result(|s| s.replace(id, current));
                        ui.close();
                    }
                    if crate::controls::menu_action(ui, "Delete").clicked() {
                        self.delete = Some(e.id);
                        ui.close();
                    }
                });
            },
        );
        // Delete asks inside the card.
        if self.delete == Some(e.id) {
            painter.rect_filled(
                preview,
                egui::CornerRadius {
                    nw: 10,
                    ne: 10,
                    sw: 0,
                    se: 0,
                },
                egui::Color32::from_black_alpha(190),
            );
            let ask = preview.shrink(12.);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(ask)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
                |ui| {
                    ui.add_space((ask.height() - 56.).max(0.) / 2.);
                    ui.label(
                        egui::RichText::new("Delete this block?")
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add(crate::controls::dialog_button("Delete", true))
                            .clicked()
                        {
                            let id = e.id;
                            self.result(|s| s.remove(id));
                            self.delete = None;
                            if self.selected == Some(id) {
                                self.selected = None;
                            }
                        }
                        if ui
                            .add(crate::controls::dialog_button("Cancel", false))
                            .clicked()
                        {
                            self.delete = None;
                        }
                    });
                },
            );
        }
        if action.is_none() && !star_response.hovered() {
            if response.double_clicked() {
                action = Some(CardAction::Insert);
            } else if response.clicked() {
                action = Some(CardAction::Select);
            }
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, selected, &e.name)
        });
        action
    }

    /// The preset library for motion, audio and frame presets (unchanged).
    fn show_library(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let response=crate::controls::Modal::new(egui::Id::unique("creative-presets-manager")).show(ctx,|ui|{
            ui.set_width((ctx.content_rect().width()-48.).clamp(260.,620.));
            crate::controls::dialog_title(ui, "Preset library");
            ui.horizontal(|ui|{
                let r=ui.add(crate::unicode_input::TextEdit::singleline(&mut self.query).hint_text("Search names or categories").desired_width((ui.available_width()-110.).max(100.)));if self.focus{r.request_focus();self.focus=false;}
                crate::controls::toggle(ui,&mut self.favorites,"Favorites");
            });
            let entries=self.store.as_ref().map(|s|s.entries().to_vec()).unwrap_or_default();
            let query=self.query.to_lowercase();let favorites=self.favorites;
            egui::ScrollArea::vertical().id_salt("preset-library-entries").max_height((ctx.content_rect().height()-230.).clamp(100.,360.)).auto_shrink([false,false]).show(ui,|ui|{
                for e in entries.iter().filter(|e|!matches!(e.payload,Payload::Group(_))&&(!favorites||e.favorite)&&(e.name.to_lowercase().contains(&query)||e.payload.category().to_lowercase().contains(&query))) {
                    ui.push_id(e.id,|ui|{
                        ui.horizontal(|ui|{
                            if favorite_button(ui,e.favorite).clicked(){self.result(|s|s.favorite(e.id,!e.favorite));}
                            ui.vertical(|ui|{ui.label(crate::panel_ui::title(&e.name,13.));ui.small(e.payload.category());});
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {ui.menu_button("…",|ui|{
                                if crate::controls::menu_action(ui,"Rename").clicked(){self.edit=Some((Some(e.id),e.name.clone(),e.payload.clone()));self.rename=true;self.return_to_library=true;self.focus=true;self.error.clear();ui.close();}
                                if crate::controls::menu_action(ui,"Duplicate").clicked(){self.result(|s|s.duplicate(e.id).map(|_|()));ui.close();}
                                if let Some(current)=self.current.clone() && current.compatible(&e.payload) && crate::controls::menu_action(ui,"Update from current settings").clicked(){self.result(|s|s.replace(e.id,current));ui.close();}
                                if crate::controls::menu_action(ui,"Delete").clicked(){self.delete=Some(e.id);ui.close();}
                            });});
                        });
                        if self.delete==Some(e.id){ui.horizontal(|ui|{ui.label("Delete this preset?");if crate::controls::menu_action(ui,"Delete").clicked(){self.result(|s|s.remove(e.id));self.delete=None;}
if ui.button("Cancel").clicked(){self.delete=None;}});}
                        crate::panel_ui::divider(ui);
                    });
                }
                if entries.iter().all(|e|matches!(e.payload,Payload::Group(_))){ui.label("Save your current audio, motion, or frame settings to start a collection.");}
            });
            if !self.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&self.error);}
            ui.add_space(8.);crate::panel_ui::divider(ui);
            ui.horizontal(|ui|{
                if ui.button("Import pack…").clicked()&&let Some(path)=rfd::FileDialog::new().add_filter("ClipDesigner preset pack",&["ccpresets"]).pick_file(){self.result(|s|s.import(&path).map(|_|()));}
                if ui.button("Export pack…").clicked()&&let Some(path)=rfd::FileDialog::new().add_filter("ClipDesigner preset pack",&["ccpresets"]).set_file_name("ClipDesigner presets.ccpresets").save_file(){self.result(|s|s.export(&path));}
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add(crate::controls::dialog_button("Done",true)).clicked(){close=true;}});
            });
        });
        if close || response.should_close() {
            self.open = false;
            self.delete = None;
        }
    }
    fn result(&mut self, f: impl FnOnce(&mut Store) -> capture_core::Result<()>) {
        self.group_thumbs.clear();
        self.error = match self.store.as_mut() {
            Some(s) => f(s).err().map(|e| e.to_string()).unwrap_or_default(),
            None => "Local preset storage is unavailable.".into(),
        };
    }
}

enum CardAction {
    Insert,
    Select,
}

/// One row of the save dialog's new-or-replace choice.
fn choice_row(ui: &mut egui::Ui, on: bool, title: &str, detail: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 8., egui::Color32::from_rgb(58, 66, 70));
    if on {
        painter.rect_stroke(
            rect,
            8.,
            egui::Stroke::new(1.5, crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    let dot = egui::pos2(rect.left() + 18., rect.top() + 16.);
    painter.circle_stroke(
        dot,
        7.,
        egui::Stroke::new(
            2.,
            if on {
                crate::shell::ACCENT
            } else {
                crate::panel_ui::MUTED
            },
        ),
    );
    if on {
        painter.circle_filled(dot, 3.5, egui::Color32::WHITE);
    }
    painter.text(
        egui::pos2(rect.left() + 34., rect.top() + 9.),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(13.),
        ui.visuals().strong_text_color(),
    );
    painter.text(
        egui::pos2(rect.left() + 34., rect.top() + 27.),
        egui::Align2::LEFT_TOP,
        detail,
        egui::FontId::proportional(11.5),
        crate::panel_ui::MUTED,
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, title));
    response
}

/// A Blocks filter chip.
fn filter_chip(ui: &mut egui::Ui, label: &str, on: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(12.5),
        egui::Color32::WHITE,
    );
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(galley.size().x + 24., 26.), egui::Sense::click());
    let fill = if on {
        crate::shell::ACCENT
    } else if response.hovered() {
        egui::Color32::from_rgb(70, 78, 82)
    } else {
        egui::Color32::from_rgb(58, 66, 70)
    };
    ui.painter().rect_filled(rect, 13., fill);
    ui.painter().galley(
        rect.center() - galley.size() / 2.,
        galley,
        if on {
            egui::Color32::WHITE
        } else {
            crate::panel_ui::MUTED
        },
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, label));
    response
}

fn favorite_button(ui: &mut egui::Ui, value: bool) -> egui::Response {
    let r = ui.add_sized([26., 26.], egui::Button::new("").selected(value));
    crate::icons::draw_scaled(
        ui.painter(),
        r.rect,
        "Favorites",
        if value {
            egui::Color32::WHITE
        } else {
            crate::panel_ui::MUTED
        },
        0.58,
    );
    r.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, true, value, "Favorite preset")
    });
    r.tip("Favorite preset")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn preset_name_focus_enter_save_and_library_preview_preserve_payloads() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
        let mut s = State::default();
        s.init();
        let payload = Payload::AudioChain(capture_core::video::effects::Chain::narration());
        s.current = Some(payload.clone());
        s.open = true;
        s.focus = true;
        s.edit = Some((None, String::new(), payload.clone()));
        let textures = std::cell::RefCell::new(std::collections::HashMap::<
            egui::TextureId,
            Vec<egui::epaint::ImageDelta>,
        >::new());
        let frame = |s: &mut State, events| {
            let mut output = PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 700.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    crate::controls::prepare_dialog_keys(ui.ctx());
                    s.show(ui.ctx());
                },
            ));
            for (id, deltas) in &output.0.textures_delta.set {
                textures
                    .borrow_mut()
                    .entry(*id)
                    .or_default()
                    .extend(deltas.clone());
            }
            output.0.textures_delta.set = textures
                .borrow()
                .iter()
                .map(|(id, deltas)| (*id, deltas.iter().cloned().collect()))
                .collect();
            output
        };
        frame(&mut s, vec![]);
        frame(&mut s, vec![]);
        assert!(ctx.text_edit_focused());
        frame(&mut s, vec![egui::Event::Text("Tutorial voice".into())]);
        frame(
            &mut s,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(s.edit.is_none(), "{}", s.error);
        assert_eq!(
            s.store.as_ref().unwrap().entries()[0].name,
            "Tutorial voice"
        );
        assert_eq!(s.store.as_ref().unwrap().entries()[0].payload, payload);
        s.store
            .as_mut()
            .unwrap()
            .save(
                "Gentle introduction",
                Payload::Motion(
                    capture_core::reusable_presets::Motion::builtins()[0]
                        .1
                        .clone(),
                ),
            )
            .unwrap();
        s.store.as_mut().unwrap().favorite(1, true).unwrap();
        let first = frame(&mut s, vec![]);
        let output = frame(&mut s, vec![]);
        if let Ok(root) = std::env::var("CLIPDESIGNER_PRESET_EVIDENCE") {
            dump_meshes(&ctx, &root, "library", first, output);
        }
        let reopen = Store::open(s.test_root.as_ref().unwrap().path()).unwrap();
        assert_eq!(reopen.entries(), s.store.as_ref().unwrap().entries());
    }

    /// Approved tab 2: Blocks as a card grid with Insert, favorites and the
    /// Recently used filter; Save as block as a compact dialog.
    #[test]
    fn blocks_grid_inserts_and_marks_use_and_save_as_block_is_compact() {
        use capture_core::group_presets::Composition;
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
        let block = |n: u32, colour: [u8; 4]| {
            let mut doc = capture_core::Document::blank(640, 400).unwrap();
            let mut badge =
                capture_core::steps::marker(1, [120., 120.], 40, colour, (n, false), [640, 400])
                    .unwrap();
            badge.id = 1;
            let mut bar = badge.clone();
            bar.id = 2;
            bar.kind = crate::Kind::Rectangle;
            bar.text = None;
            bar.start = [160., 104.];
            bar.end = [360., 136.];
            bar.fill = Some(colour);
            doc.annotations = vec![badge, bar];
            Payload::Group(Composition::capture(&doc, &[1, 2]).unwrap())
        };
        let mut s = State::default();
        s.init();
        let store = s.store.as_mut().unwrap();
        store
            .save("Step + bar", block(1, [24, 145, 193, 255]))
            .unwrap();
        store
            .save("Warning bar", block(2, [227, 162, 59, 255]))
            .unwrap();
        store
            .save("Done marker", block(3, [79, 194, 138, 255]))
            .unwrap();
        store.favorite(2, true).unwrap();
        s.open_groups(None);
        let size = egui::vec2(1000., 760.);
        let textures = std::cell::RefCell::new(std::collections::HashMap::<
            egui::TextureId,
            Vec<egui::epaint::ImageDelta>,
        >::new());
        let frame = |s: &mut State, events| {
            let mut output = PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    crate::controls::prepare_dialog_keys(ui.ctx());
                    s.show(ui.ctx());
                },
            ));
            for (id, deltas) in &output.0.textures_delta.set {
                textures
                    .borrow_mut()
                    .entry(*id)
                    .or_default()
                    .extend(deltas.clone());
            }
            output.0.textures_delta.set = textures
                .borrow()
                .iter()
                .map(|(id, deltas)| (*id, deltas.iter().cloned().collect()))
                .collect();
            output
        };
        frame(&mut s, vec![]);
        frame(&mut s, vec![]);
        s.selected = Some(2);
        let first = frame(&mut s, vec![]);
        let grid = frame(&mut s, vec![]);
        if let Ok(root) = std::env::var("CLIPDESIGNER_PRESET_EVIDENCE") {
            dump_meshes(&ctx, &root, "blocks", first, grid);
        }
        // The footer's Insert places the selected block and records its use.
        s.selected = Some(3);
        let id = 3;
        if let Some(Payload::Group(c)) = s
            .store
            .as_ref()
            .unwrap()
            .entries()
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.payload.clone())
        {
            s.placed_group = Some(c);
            s.result(|st| st.mark_used(id));
        }
        assert!(s.take_group().is_some());
        let used: Vec<_> = s
            .store
            .as_ref()
            .unwrap()
            .entries()
            .iter()
            .filter(|e| e.used > 0)
            .map(|e| e.id)
            .collect();
        assert_eq!(used, vec![3], "Recently used lists the inserted block");
        // Save as block opens compact.
        s.save_payload(block(4, [120, 90, 200, 255]));
        frame(&mut s, vec![]);
        frame(&mut s, vec![]);
        let first = frame(&mut s, vec![]);
        let save = frame(&mut s, vec![]);
        let dialog = save
            .0
            .shapes
            .iter()
            .map(|c| c.shape.visual_bounding_rect())
            .filter(|r| r.is_finite() && r.width() < size.x - 1.)
            .fold(egui::Rect::NOTHING, |a, b| a.union(b));
        if let Ok(root) = std::env::var("CLIPDESIGNER_PRESET_EVIDENCE") {
            dump_meshes(&ctx, &root, "save-as-block", first, save);
        }
        assert!(dialog.width() <= 480., "save dialog is compact: {dialog:?}");
        assert!(
            dialog.height() <= 420.,
            "save dialog fits its content: {dialog:?}"
        );
    }
}
