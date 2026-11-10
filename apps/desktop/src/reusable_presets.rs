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
}

impl State {
    pub(crate) fn open_groups(&mut self) {
        self.init();
        self.group_only = true;
        self.current = None;
        self.query.clear();
        self.open = true;
        self.focus = true;
    }
    /// The payload waiting in the save dialog, for tests.
    #[cfg(test)]
    pub(crate) fn pending_save(&self) -> Option<&Payload> {
        self.edit.as_ref().map(|(_, _, p)| p)
    }
    pub(crate) fn take_group(&mut self) -> Option<capture_core::group_presets::Composition> {
        self.placed_group.take()
    }
    pub(crate) fn save_payload(&mut self, payload: Payload) {
        self.init();
        self.current = Some(payload.clone());
        self.edit = Some((None, String::new(), payload));
        self.focus = true;
        self.open = true;
        self.error.clear();
    }
    fn group_thumb(
        &mut self,
        ui: &mut egui::Ui,
        id: u64,
        c: &capture_core::group_presets::Composition,
    ) {
        if let std::collections::btree_map::Entry::Vacant(entry) = self.group_thumbs.entry(id) {
            let mut doc = capture_core::Document::blank(
                c.objects.source_canvas[0],
                c.objects.source_canvas[1],
            )
            .unwrap();
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
            let scale =
                (100. / (hi[0] - lo[0]).max(1.) as f64).min(64. / (hi[1] - lo[1]).max(1.) as f64);
            if let Ok(list) = doc.display_list()
                && let Ok(p) = list.render_viewport(capture_core::display::Viewport {
                    size: [112, 76],
                    origin: [lo[0] as f64 - 6. / scale, lo[1] as f64 - 6. / scale],
                    scale,
                })
            {
                entry.insert(ui.ctx().load_texture(
                    format!("group-preset-{id}"),
                    egui::ColorImage::from_rgba_unmultiplied([112, 76], p.as_raw()),
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        if let Some(t) = self.group_thumbs.get(&id) {
            ui.image((t.id(), egui::vec2(112., 76.)));
        }
    }

    fn init(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        #[cfg(not(test))]
        let root =
            crate::storage_locations::configured(capture_core::quick_preferences::Folder::Presets)
                .or_else(|| std::env::var_os("CLIPCRAFT_STYLE_DIR").map(std::path::PathBuf::from))
                .or_else(|| {
                    std::env::var_os("LOCALAPPDATA").map(|p| {
                        std::path::PathBuf::from(p)
                            .join("ClipCraft")
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
        let mut close = false;
        let mut save = false;
        let mut cancel_edit = false;
        let response=crate::controls::Modal::new(egui::Id::unique("creative-presets-manager")).show(ctx,|ui|{
            ui.set_width((ctx.content_rect().width()-48.).clamp(260.,620.));
            ui.label(crate::panel_ui::title(if self.edit.is_some(){"Save preset"}else if self.group_only{"Blocks"}else{"Preset library"},20.));
            crate::panel_ui::divider(ui);
            if let Some((id,label,payload))=&mut self.edit {
                ui.label(format!("{} · Name",payload.category()));
                let r=ui.add(crate::unicode_input::TextEdit::singleline(label).char_limit(80).desired_width(f32::INFINITY));if self.focus {r.request_focus();self.focus=false;}
                ui.add_space(10.);
                ui.label(crate::panel_ui::title("Compatible presets",13.));
                if ui.selectable_label(id.is_none(),"+ Save as a new preset").clicked(){*id=None;label.clear();self.focus=true;}
                let compatible=self.store.as_ref().map(|s|s.entries().iter().filter(|e|e.payload.compatible(payload)).cloned().collect::<Vec<_>>()).unwrap_or_default();
                egui::ScrollArea::vertical().max_height(220.).id_salt("save-compatible-presets").show(ui,|ui|{
                    for e in compatible {if ui.selectable_label(*id==Some(e.id),&e.name).clicked(){*id=Some(e.id);*label=e.name;}}
                });
                ui.weak(if id.is_some(){"Save replaces this preset with the current settings."}else{"A new preset will be added to your library."});
                if !self.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&self.error);}
                ui.add_space(12.);ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    if ui.add(crate::controls::dialog_button("Cancel",false)).clicked(){cancel_edit=true;}
                    if ui.add_enabled(!label.trim().is_empty(),crate::controls::dialog_button("Save",true)).clicked(){save=true;}
                });
            }else{
                ui.horizontal(|ui|{
                    let r=ui.add(crate::unicode_input::TextEdit::singleline(&mut self.query).hint_text("Search names or categories").desired_width((ui.available_width()-110.).max(100.)));if self.focus{r.request_focus();self.focus=false;}
                    crate::controls::toggle(ui,&mut self.favorites,"Favorites");
                });
                let entries=self.store.as_ref().map(|s|s.entries().to_vec()).unwrap_or_default();
                let query=self.query.to_lowercase();let favorites=self.favorites;let group_only=self.group_only;
                egui::ScrollArea::vertical().id_salt("preset-library-entries").max_height((ctx.content_rect().height()-230.).clamp(100.,360.)).auto_shrink([false,false]).show(ui,|ui|{
                    for e in entries.iter().filter(|e|(!group_only||matches!(e.payload,Payload::Group(_)))&&(!favorites||e.favorite)&&(e.name.to_lowercase().contains(&query)||e.payload.category().to_lowercase().contains(&query))) {
                        ui.push_id(e.id,|ui|{
                            ui.horizontal(|ui|{
                                if let Payload::Group(c)=&e.payload{self.group_thumb(ui,e.id,c);if ui.add(crate::controls::dialog_button("Insert",true)).clicked(){self.placed_group=Some(c.clone());close=true;}}
                                if favorite_button(ui,e.favorite).clicked(){self.result(|s|s.favorite(e.id,!e.favorite));}
                                ui.vertical(|ui|{ui.label(crate::panel_ui::title(&e.name,13.));ui.small(e.payload.category());});
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {ui.menu_button("…",|ui|{
                                    if crate::controls::menu_action(ui,"Rename").clicked(){self.edit=Some((Some(e.id),e.name.clone(),e.payload.clone()));self.focus=true;self.error.clear();ui.close();}
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
                    if entries.is_empty(){ui.label("Save your current audio, motion, or frame settings to start a collection.");}
                });
                if !self.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&self.error);}
                ui.add_space(8.);crate::panel_ui::divider(ui);
                ui.horizontal(|ui|{
                    if ui.button("Import pack…").clicked()&&let Some(path)=rfd::FileDialog::new().add_filter("ClipCraft preset pack",&["ccpresets"]).pick_file(){self.result(|s|s.import(&path).map(|_|()));}
                    if ui.button("Export pack…").clicked()&&let Some(path)=rfd::FileDialog::new().add_filter("ClipCraft preset pack",&["ccpresets"]).set_file_name("ClipCraft presets.ccpresets").save_file(){self.result(|s|s.export(&path));}
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add(crate::controls::dialog_button("Done",true)).clicked(){close=true;}});
                });
            }
        });
        if save && let Some((id, name, payload)) = self.edit.clone() {
            let result = self
                .store
                .as_mut()
                .ok_or_else(|| "Local preset storage is unavailable.".to_string())
                .and_then(|s| {
                    if let Some(id) = id {
                        s.update(id, &name, payload).map_err(|e| e.to_string())
                    } else {
                        s.save(&name, payload)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }
                });
            match result {
                Ok(()) => {
                    self.edit = None;
                    self.group_thumbs.clear();
                    self.query.clear();
                    self.error.clear();
                }
                Err(e) => self.error = e,
            }
        }
        if cancel_edit {
            self.edit = None;
            self.error.clear();
        }
        if close || response.should_close() {
            self.open = false;
            self.edit = None;
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
        if let Ok(root) = std::env::var("CLIPCRAFT_PRESET_EVIDENCE") {
            dump_meshes(&ctx, &root, "library", first, output);
        }
        let reopen = Store::open(s.test_root.as_ref().unwrap().path()).unwrap();
        assert_eq!(reopen.entries(), s.store.as_ref().unwrap().entries());
    }
}
