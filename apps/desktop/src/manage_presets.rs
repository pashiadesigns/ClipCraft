//! Manage presets and Remove a kit (approved removing presets mockup):
//! everything you've saved in one list (styles, arrow styles, Blocks,
//! motion/audio/frame presets, palettes, custom sensitive types,
//! watermarks, templates) with search, a kind filter, tick boxes, Delete
//! selected and Export selected, where each item came from, and the kits
//! you added with Remove.
use crate::guided::Restore;
use crate::tips::Tip as _;
use crate::{Editor, egui};
use capture_core::quick_preferences::KitItem;
use std::collections::BTreeSet;

/// One saved preset, wherever it is kept.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Item {
    Style(String, String),
    Arrow(String),
    Entry(u64),
    Palette(String),
    Type(String),
    Watermark(String),
    Template(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub item: Item,
    pub name: String,
    pub kind: String,
    pub fingerprint: u64,
    pub color: Option<[u8; 4]>,
}

/// FNV-1a: a fingerprint that stays the same from run to run.
fn fingerprint(value: &impl serde::Serialize) -> u64 {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[derive(Clone, Default)]
struct Window {
    query: String,
    kind: String,
    ticked: BTreeSet<Item>,
    /// A kit being removed, and whether to keep items changed since.
    removing: Option<(String, bool)>,
    error: String,
}
/// The "Manage presets…" link under a preset gallery (outside its scroll
/// area, so the gallery's height and scrollbar don't change). True when clicked.
pub(crate) fn manage_link(ui: &mut egui::Ui) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new("Manage presets\u{2026}").color(crate::shell::ACCENT))
            .frame(false),
    )
    .tip("Manage presets \u{b7} See every preset you saved, delete or export several at once, and remove a kit you added.")
    .clicked()
}

fn window_id() -> egui::Id {
    egui::Id::unique("manage-presets-window")
}

impl Editor {
    pub(crate) fn open_manage_presets(&mut self) {
        let ctx = self.presets_ctx.clone();
        if let Some(ctx) = ctx {
            ctx.data_mut(|d| d.insert_temp(window_id(), Window::default()));
        }
    }
    /// Every saved preset, with its kind and fingerprint.
    pub(crate) fn preset_rows(&mut self) -> Vec<Row> {
        let mut rows = vec![];
        if let Some(store) = self.presets.templates.as_ref() {
            for s in store.entries() {
                rows.push(Row {
                    item: Item::Style(format!("{:?}", s.kind), s.name.clone()),
                    name: s.name.clone(),
                    kind: "Style".into(),
                    fingerprint: fingerprint(s),
                    color: Some(s.color),
                });
            }
        }
        if let Some(store) = self.presets.arrow_store() {
            for s in store.entries() {
                rows.push(Row {
                    item: Item::Arrow(s.name.clone()),
                    name: s.name.clone(),
                    kind: "Arrow style".into(),
                    fingerprint: fingerprint(s),
                    color: Some(s.color),
                });
            }
        }
        if let Some(store) = self.presets.reusable.store_mut() {
            for e in store.entries() {
                rows.push(Row {
                    item: Item::Entry(e.id),
                    name: e.name.clone(),
                    kind: e.payload.category().to_owned(),
                    fingerprint: fingerprint(&e.payload),
                    color: None,
                });
            }
        }
        if let Some(p) = crate::color_widgets::palette_store().and_then(|s| s.read().ok()) {
            for palette in &p.palettes {
                rows.push(Row {
                    item: Item::Palette(palette.name.clone()),
                    name: palette.name.clone(),
                    kind: "Palette".into(),
                    fingerprint: fingerprint(&palette.colors),
                    color: palette.colors.first().copied(),
                });
            }
        }
        for t in self.quick_controls.sensitive_types() {
            rows.push(Row {
                item: Item::Type(t.name.clone()),
                name: t.name.clone(),
                kind: "Sensitive type".into(),
                fingerprint: fingerprint(&t),
                color: None,
            });
        }
        for w in self.quick_controls.watermarks().0 {
            rows.push(Row {
                item: Item::Watermark(w.name.clone()),
                name: w.name.clone(),
                kind: "Watermark".into(),
                fingerprint: fingerprint(&w),
                color: Some(w.color),
            });
        }
        if let Some(root) = self.template_root()
            && let Ok(library) = capture_core::templates::library::Library::open(&root)
        {
            for e in library.0.entries() {
                rows.push(Row {
                    item: Item::Template(e.id.clone()),
                    name: e.label.clone(),
                    kind: "Template".into(),
                    fingerprint: fingerprint(&(e.updated_ms, e.created_ms)),
                    color: None,
                });
            }
        }
        rows
    }
    /// Deletes one preset; what Undo needs to put it back (templates can't
    /// be put back).
    fn delete_preset(&mut self, item: &Item) -> Result<Option<Restore>, String> {
        let err = |e: capture_core::Error| e.to_string();
        match item {
            Item::Style(kind, name) => {
                let store = self
                    .presets
                    .templates
                    .as_mut()
                    .ok_or("Styles aren't available.")?;
                let style = store
                    .entries()
                    .iter()
                    .find(|s| format!("{:?}", s.kind) == *kind && s.name == *name)
                    .cloned()
                    .ok_or("That style is already gone.")?;
                store.remove(style.kind, name).map_err(err)?;
                Ok(Some(Restore::ObjectStyle(style)))
            }
            Item::Arrow(name) => {
                let store = self
                    .presets
                    .arrow_store()
                    .ok_or("Arrow styles aren't available.")?;
                let style = store
                    .entries()
                    .iter()
                    .find(|s| s.name == *name)
                    .cloned()
                    .ok_or("That arrow style is already gone.")?;
                store.remove(name).map_err(err)?;
                Ok(Some(Restore::Arrow(style)))
            }
            Item::Entry(id) => {
                let store = self
                    .presets
                    .reusable
                    .store_mut()
                    .ok_or("Presets aren't available.")?;
                let e = store
                    .entries()
                    .iter()
                    .find(|e| e.id == *id)
                    .cloned()
                    .ok_or("That preset is already gone.")?;
                store.remove(*id).map_err(err)?;
                Ok(Some(Restore::Entry(e.name, e.payload)))
            }
            Item::Palette(name) => {
                use capture_core::palettes::Edit;
                let store =
                    crate::color_widgets::palette_store().ok_or("Palettes aren't available.")?;
                let before = store.read().map_err(err)?;
                let palette = before
                    .palettes
                    .iter()
                    .find(|p| p.name == *name)
                    .cloned()
                    .ok_or("That palette is already gone.")?;
                store.edit(Edit::Select(name.clone())).map_err(err)?;
                store.edit(Edit::Delete).map_err(err)?;
                if before.selected != *name {
                    let _ = store.edit(Edit::Select(before.selected.clone()));
                }
                if let Some(ctx) = &self.presets_ctx {
                    crate::color_widgets::reload_palettes(ctx);
                }
                Ok(Some(Restore::Palette(palette.name, palette.colors)))
            }
            Item::Type(name) => {
                let mut list = self.quick_controls.sensitive_types();
                let i = list
                    .iter()
                    .position(|t| t.name == *name)
                    .ok_or("That type is already gone.")?;
                let t = list.remove(i);
                self.quick_controls.set_sensitive_types(list)?;
                Ok(Some(Restore::SensitiveType(t)))
            }
            Item::Watermark(name) => {
                let (mut list, default) = self.quick_controls.watermarks();
                let i = list
                    .iter()
                    .position(|w| w.name == *name)
                    .ok_or("That watermark is already gone.")?;
                let w = list.remove(i);
                let default = default.filter(|d| d != name);
                self.quick_controls.remember_watermarks(list, default);
                Ok(Some(Restore::Watermark(w)))
            }
            Item::Template(id) => {
                let root = self.template_root().ok_or("Templates aren't available.")?;
                let mut library =
                    capture_core::templates::library::Library::open(&root).map_err(err)?;
                let entry = library
                    .0
                    .entries()
                    .iter()
                    .find(|e| e.id == *id)
                    .cloned()
                    .ok_or("That template is already gone.")?;
                library.0.discard(&entry).map_err(err)?;
                if let Some(ctx) = &self.presets_ctx {
                    crate::template_picker::forget_catalog(ctx);
                }
                Ok(None)
            }
        }
    }
    /// Deletes presets, then offers Undo in the message bar.
    pub(crate) fn delete_presets(&mut self, items: &[Item], what: &str) {
        let mut undo = vec![];
        let mut failed = None;
        for item in items {
            match self.delete_preset(item) {
                Ok(Some(r)) => undo.push(r),
                Ok(None) => {}
                Err(e) => failed = Some(e),
            }
        }
        match failed {
            Some(e) => self.fail(e),
            None if undo.is_empty() => self.status = format!("Deleted {what}."),
            None => self.done_with(
                format!("Deleted {what}."),
                "Undo",
                crate::guided::Action::Restore(undo),
            ),
        }
    }
    /// Undo: puts deleted presets back (and shows hidden ones again).
    pub(crate) fn restore_presets(&mut self, items: Vec<Restore>) {
        let mut failed = None;
        for r in items {
            let result: Result<(), String> = match r {
                Restore::ObjectStyle(s) => self
                    .presets
                    .templates
                    .as_mut()
                    .ok_or_else(|| "Styles aren't available.".to_owned())
                    .and_then(|store| store.save(s).map_err(|e| e.to_string())),
                Restore::Arrow(s) => self
                    .presets
                    .arrow_store()
                    .ok_or_else(|| "Arrow styles aren't available.".to_owned())
                    .and_then(|store| store.save(s).map_err(|e| e.to_string())),
                Restore::Entry(name, payload) => self
                    .presets
                    .reusable
                    .store_mut()
                    .ok_or_else(|| "Presets aren't available.".to_owned())
                    .and_then(|store| {
                        store
                            .save(&name, payload)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }),
                Restore::Palette(name, colors) => crate::color_widgets::palette_store()
                    .ok_or_else(|| "Palettes aren't available.".to_owned())
                    .and_then(|store| {
                        let before = store.read().map_err(|e| e.to_string())?;
                        store
                            .edit(capture_core::palettes::Edit::Create { name, colors })
                            .map_err(|e| e.to_string())?;
                        let _ = store.edit(capture_core::palettes::Edit::Select(before.selected));
                        Ok(())
                    }),
                Restore::SensitiveType(t) => {
                    let mut list = self.quick_controls.sensitive_types();
                    list.push(t);
                    self.quick_controls.set_sensitive_types(list)
                }
                Restore::Watermark(w) => {
                    let (mut list, default) = self.quick_controls.watermarks();
                    list.push(w);
                    self.quick_controls.remember_watermarks(list, default);
                    Ok(())
                }
                Restore::Hidden(key) => {
                    let mut records = self.quick_controls.preset_records();
                    records.hidden.retain(|h| *h != key);
                    self.quick_controls.set_preset_records(records)
                }
            };
            if let Err(e) = result {
                failed = Some(e);
            }
        }
        if let Some(ctx) = &self.presets_ctx {
            crate::color_widgets::reload_palettes(ctx);
        }
        match failed {
            Some(e) => self.fail(e),
            None => self.status = "Put back.".into(),
        }
    }
    /// Hides a built-in (or kit) preset from its gallery; Undo shows it.
    pub(crate) fn hide_preset(&mut self, key: String, name: &str) {
        let mut records = self.quick_controls.preset_records();
        if !records.hidden.contains(&key) {
            records.hidden.push(key.clone());
        }
        match self.quick_controls.set_preset_records(records) {
            Ok(()) => self.done_with(
                format!("Hid \u{201c}{name}\u{201d}. Show hidden brings it back."),
                "Undo",
                crate::guided::Action::Restore(vec![Restore::Hidden(key)]),
            ),
            Err(e) => self.fail(e),
        }
    }
    /// Records what a kit added, from the presets before and after.
    pub(crate) fn record_kit(&mut self, kit: &str, before: &[Row]) {
        let after = self.preset_rows();
        let added: Vec<KitItem> = after
            .iter()
            .filter(|r| {
                !before
                    .iter()
                    .any(|b| b.kind == r.kind && b.name == r.name && b.fingerprint == r.fingerprint)
            })
            .map(|r| KitItem {
                kind: r.kind.clone(),
                name: r.name.clone(),
                fingerprint: r.fingerprint,
            })
            .collect();
        if added.is_empty() {
            return;
        }
        let mut records = self.quick_controls.preset_records();
        let list = records.kits.entry(kit.to_owned()).or_default();
        for item in added {
            list.retain(|i| !(i.kind == item.kind && i.name == item.name));
            list.push(item);
        }
        let _ = self.quick_controls.set_preset_records(records);
    }
    /// The kit a preset came from, if a kit added it.
    pub(crate) fn kit_of(&self, kind: &str, name: &str) -> Option<String> {
        self.quick_controls
            .preset_records()
            .kits
            .iter()
            .find(|(_, items)| items.iter().any(|i| i.kind == kind && i.name == name))
            .map(|(k, _)| k.clone())
    }
    /// Remove a kit: deletes what it added (keeping items changed since
    /// when asked), with Undo.
    pub(crate) fn remove_kit(&mut self, kit: &str, keep_changed: bool) {
        let mut records = self.quick_controls.preset_records();
        let Some(items) = records.kits.remove(kit) else {
            return;
        };
        let rows = self.preset_rows();
        let doomed: Vec<Item> = items
            .iter()
            .filter_map(|i| {
                rows.iter()
                    .find(|r| r.kind == i.kind && r.name == i.name)
                    .filter(|r| !keep_changed || r.fingerprint == i.fingerprint)
                    .map(|r| r.item.clone())
            })
            .collect();
        let _ = self.quick_controls.set_preset_records(records);
        let n = doomed.len();
        self.delete_presets(
            &doomed,
            &format!(
                "{n} item{} from \u{201c}{kit}\u{201d}",
                if n == 1 { "" } else { "s" }
            ),
        );
    }
    /// The kits you added, each with Remove… (shared by Manage presets and
    /// Preferences › Team). Returns the kit to remove and whether to keep
    /// items changed since.
    pub(crate) fn kit_list(
        &mut self,
        ui: &mut egui::Ui,
        removing: &mut Option<(String, bool)>,
    ) -> Option<(String, bool)> {
        let records = self.quick_controls.preset_records();
        let mut chosen = None;
        if records.kits.is_empty() {
            ui.weak("No kits added yet.");
            return None;
        }
        let rows = self.preset_rows();
        let mut cancel = false;
        for (kit, items) in &records.kits {
            let present: Vec<&KitItem> = items
                .iter()
                .filter(|i| rows.iter().any(|r| r.kind == i.kind && r.name == i.name))
                .collect();
            let changed = present
                .iter()
                .filter(|i| {
                    rows.iter().any(|r| {
                        r.kind == i.kind && r.name == i.name && r.fingerprint != i.fingerprint
                    })
                })
                .count();
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(40, 48, 51))
                .corner_radius(7)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(kit).strong());
                            ui.weak(format!(
                                "{} item{} added",
                                present.len(),
                                if present.len() == 1 { "" } else { "s" }
                            ));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if removing.as_ref().is_none_or(|(k, _)| k != kit)
                                && ui.button("Remove\u{2026}").clicked()
                            {
                                *removing = Some((kit.clone(), true));
                            }
                        });
                    });
                    if let Some((k, keep)) = removing.as_mut()
                        && k == kit
                    {
                        ui.label(egui::RichText::new("Remove what this kit added?").strong());
                        if changed > 0 {
                            crate::controls::toggle(
                                ui,
                                keep,
                                &format!(
                                    "Keep the {changed} item{} you changed since",
                                    if changed == 1 { "" } else { "s" }
                                ),
                            );
                        }
                        ui.horizontal(|ui| {
                            let n = present.len() - if *keep { changed } else { 0 };
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(format!(
                                            "Remove {n} item{}",
                                            if n == 1 { "" } else { "s" }
                                        ))
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(egui::Color32::from_rgb(142, 58, 52)),
                                )
                                .clicked()
                            {
                                chosen = Some((kit.clone(), *keep));
                            }
                            if ui.button("Cancel").clicked() {
                                cancel = true;
                            }
                        });
                    }
                });
        }
        if cancel {
            *removing = None;
        }
        chosen
    }
    /// The Manage presets window.
    pub(crate) fn show_manage_presets(&mut self, ctx: &egui::Context) {
        self.presets_ctx = Some(ctx.clone());
        let Some(mut w) = ctx.data(|d| d.get_temp::<Window>(window_id())) else {
            return;
        };
        let rows = self.preset_rows();
        let mut close = false;
        let mut delete = false;
        let mut export = false;
        let mut remove_kit = None;
        let kinds: Vec<String> = {
            let mut k: Vec<String> = rows.iter().map(|r| r.kind.clone()).collect();
            k.sort();
            k.dedup();
            k
        };
        let response =
            crate::controls::Modal::new(egui::Id::unique("manage-presets")).show(ctx, |ui| {
                let width = 900_f32.min(ctx.content_rect().width() - 96.).max(520.);
                ui.set_width(width);
                crate::controls::dialog_title(ui, "Manage presets");
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        crate::unicode_input::TextEdit::singleline(&mut w.query)
                            .hint_text("Search names")
                            .desired_width(220.),
                    );
                    crate::controls::segments_wrapped(ui, |ui| {
                        if ui
                            .add(egui::Button::selectable(w.kind.is_empty(), "All"))
                            .clicked()
                        {
                            w.kind.clear();
                        }
                        for k in &kinds {
                            if ui
                                .add(egui::Button::selectable(w.kind == *k, k.as_str()))
                                .clicked()
                            {
                                w.kind = k.clone();
                            }
                        }
                    });
                });
                ui.add_space(6.);
                let query = w.query.trim().to_lowercase();
                let shown: Vec<&Row> = rows
                    .iter()
                    .filter(|r| {
                        (w.kind.is_empty() || r.kind == w.kind)
                            && (query.is_empty() || r.name.to_lowercase().contains(&query))
                    })
                    .collect();
                let height = (ctx.content_rect().height() - 330.).clamp(180., 380.);
                egui::ScrollArea::vertical()
                    .id_salt("manage-presets-rows")
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for r in &shown {
                            ui.horizontal(|ui| {
                                let mut on = w.ticked.contains(&r.item);
                                if ui.checkbox(&mut on, "").changed() {
                                    if on {
                                        w.ticked.insert(r.item.clone());
                                    } else {
                                        w.ticked.remove(&r.item);
                                    }
                                }
                                let (sw, _) = ui.allocate_exact_size(
                                    egui::vec2(30., 18.),
                                    egui::Sense::hover(),
                                );
                                if let Some(c) = r.color {
                                    ui.painter().rect_filled(
                                        sw,
                                        3.,
                                        egui::Color32::from_rgba_unmultiplied(
                                            c[0],
                                            c[1],
                                            c[2],
                                            c[3].max(60),
                                        ),
                                    );
                                }
                                ui.add_sized(
                                    [width * 0.36, 20.],
                                    egui::Label::new(&r.name).truncate(),
                                );
                                ui.add_sized(
                                    [130., 20.],
                                    egui::Label::new(egui::RichText::new(&r.kind).size(11.5)),
                                );
                                let from = self.kit_of(&r.kind, &r.name).map_or_else(
                                    || "Saved by you".to_owned(),
                                    |k| format!("From kit: {k}"),
                                );
                                ui.weak(from);
                            });
                        }
                        if shown.is_empty() {
                            ui.weak(if rows.is_empty() {
                                "Nothing saved yet."
                            } else {
                                "Nothing matches."
                            });
                        }
                    });
                crate::controls::dialog_section(ui, "Kits you added", None);
                let mut removing = w.removing.clone();
                if let Some(k) = self.kit_list(ui, &mut removing) {
                    remove_kit = Some(k);
                    removing = None;
                }
                w.removing = removing;
                if !w.error.is_empty() {
                    crate::guided::dialog_message(ui, crate::guided::Kind::Error, &w.error, false);
                }
                crate::panel_ui::divider(ui);
                ui.horizontal(|ui| {
                    ui.weak(format!(
                        "{} saved \u{b7} {} selected",
                        rows.len(),
                        w.ticked.len()
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(crate::controls::dialog_button("Done", true))
                            .clicked()
                        {
                            close = true;
                        }
                        if ui
                            .add_enabled(
                                !w.ticked.is_empty(),
                                egui::Button::new("Export selected\u{2026}"),
                            )
                            .tip("Export selected \u{b7} Save the ticked presets as a kit to send.")
                            .clicked()
                        {
                            export = true;
                        }
                        let n = w.ticked.len();
                        if ui
                            .add_enabled(
                                n > 0,
                                egui::Button::new(
                                    egui::RichText::new(if n > 0 {
                                        format!("Delete {n} selected")
                                    } else {
                                        "Delete selected".into()
                                    })
                                    .color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_rgb(142, 58, 52)),
                            )
                            .clicked()
                        {
                            delete = true;
                        }
                    });
                });
            });
        if delete {
            let items: Vec<Item> = w.ticked.iter().cloned().collect();
            let n = items.len();
            self.delete_presets(
                &items,
                &format!("{n} preset{}", if n == 1 { "" } else { "s" }),
            );
            w.ticked.clear();
        }
        if let Some((kit, keep)) = remove_kit {
            self.remove_kit(&kit, keep);
        }
        if export {
            let kit = self.kit_of_items(&w.ticked);
            self.suspend_global_capture();
            if let Some(mut path) = rfd::FileDialog::new()
                .add_filter("ClipDesigner kit", &[capture_core::kit::EXTENSION])
                .set_file_name(format!("Presets.{}", capture_core::kit::EXTENSION))
                .save_file()
            {
                if path.extension().is_none() {
                    path.set_extension(capture_core::kit::EXTENSION);
                }
                match kit.write(&path) {
                    Ok(()) => {
                        self.status =
                            format!("Exported {} presets to {}", w.ticked.len(), path.display())
                    }
                    Err(e) => w.error = crate::guided::friendly(&e.to_string()),
                }
            }
        }
        if close || response.should_close() {
            ctx.data_mut(|d| d.remove::<Window>(window_id()));
        } else {
            ctx.data_mut(|d| d.insert_temp(window_id(), w));
        }
    }
    /// A kit holding the ticked presets.
    fn kit_of_items(&mut self, items: &BTreeSet<Item>) -> capture_core::kit::Kit {
        let mut all = self.gather_kit("Presets");
        all.presets
            .retain(|s| items.contains(&Item::Style(format!("{:?}", s.kind), s.name.clone())));
        all.arrow_styles
            .retain(|s| items.contains(&Item::Arrow(s.name.clone())));
        let entries: Vec<(u64, String)> = self
            .presets
            .reusable
            .store_mut()
            .map(|s| s.entries().iter().map(|e| (e.id, e.name.clone())).collect())
            .unwrap_or_default();
        all.creative.retain(|c| {
            entries
                .iter()
                .any(|(id, n)| *n == c.name && items.contains(&Item::Entry(*id)))
        });
        all.palettes
            .retain(|p| items.contains(&Item::Palette(p.name.clone())));
        all.sensitive_types
            .retain(|t| items.contains(&Item::Type(t.name.clone())));
        all.watermarks
            .retain(|w| items.contains(&Item::Watermark(w.name.clone())));
        let names: Vec<String> = self
            .preset_rows()
            .into_iter()
            .filter(|r| matches!(r.item, Item::Template(_)) && items.contains(&r.item))
            .map(|r| r.name)
            .collect();
        all.templates.retain(|t| names.contains(&t.name));
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capture_core::privacy::custom::CustomType;

    fn editor(ctx: &egui::Context, dir: &std::path::Path) -> Editor {
        let mut e = crate::canvas::tests::editor(ctx);
        e.presets.templates = Some(capture_core::object_styles::Store::open(dir).unwrap());
        e.quick_controls.use_store_for_test(dir);
        e
    }
    fn style(name: &str) -> capture_core::object_styles::ObjectStyle {
        let mut s = capture_core::object_styles::presets(crate::Kind::Rectangle)[0].clone();
        s.name = name.into();
        s
    }

    /// Delete selected presets of different kinds, then Undo puts them back.
    #[test]
    fn deleting_presets_offers_undo_that_puts_them_back() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let mut e = editor(&ctx, dir.path());
        e.presets_ctx = Some(ctx.clone());
        e.presets
            .templates
            .as_mut()
            .unwrap()
            .save(style("Blue box"))
            .unwrap();
        e.quick_controls
            .set_sensitive_types(vec![CustomType::new("Staff ID", "[WE]#{5}")])
            .unwrap();
        let items = vec![
            Item::Style("Rectangle".into(), "Blue box".into()),
            Item::Type("Staff ID".into()),
        ];
        e.delete_presets(&items, "2 presets");
        assert!(e.presets.templates.as_ref().unwrap().entries().is_empty());
        assert!(e.quick_controls.sensitive_types().is_empty());
        assert_eq!(e.notice.kind, crate::guided::Kind::Done);
        let Some((label, action)) = e.notice.action.clone() else {
            panic!("no Undo offered");
        };
        assert_eq!(label, "Undo");
        e.run_fix(action, &ctx);
        assert_eq!(
            e.presets.templates.as_ref().unwrap().entries()[0].name,
            "Blue box"
        );
        assert_eq!(e.quick_controls.sensitive_types()[0].name, "Staff ID");
    }

    /// Adding a kit records what it added; Remove a kit takes those out,
    /// keeping an item changed since when asked.
    #[test]
    fn a_kit_can_be_removed_keeping_items_changed_since() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let mut e = editor(&ctx, dir.path());
        e.presets_ctx = Some(ctx.clone());
        e.presets
            .templates
            .as_mut()
            .unwrap()
            .save(style("Mine"))
            .unwrap();
        let mut kit = capture_core::kit::Kit::new("Team styles", "test");
        kit.presets = vec![style("Team blue"), style("Team red")];
        kit.sensitive_types = vec![CustomType::new("Ticket", "\"T-\"#{4}")];
        e.add_kit(&ctx, &kit, [true; 6], false).unwrap();
        let records = e.quick_controls.preset_records();
        assert_eq!(records.kits["Team styles"].len(), 3, "{records:?}");
        assert_eq!(
            e.kit_of("Style", "Team blue").as_deref(),
            Some("Team styles")
        );
        assert_eq!(e.kit_of("Style", "Mine"), None);
        // Change one of the kit's styles, then remove the kit keeping it.
        let mut changed = style("Team red");
        changed.width = 9;
        e.presets.templates.as_mut().unwrap().save(changed).unwrap();
        e.remove_kit("Team styles", true);
        let names: Vec<String> = e
            .presets
            .templates
            .as_ref()
            .unwrap()
            .entries()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(
            names.contains(&"Mine".to_owned()) && names.contains(&"Team red".to_owned()),
            "{names:?}"
        );
        assert!(!names.contains(&"Team blue".to_owned()));
        assert!(e.quick_controls.sensitive_types().is_empty());
        assert!(e.quick_controls.preset_records().kits.is_empty());
        // Undo brings the removed items back.
        let (_, action) = e.notice.action.clone().unwrap();
        e.run_fix(action, &ctx);
        assert!(
            e.presets
                .templates
                .as_ref()
                .unwrap()
                .entries()
                .iter()
                .any(|s| s.name == "Team blue")
        );
    }

    /// Hiding a built-in, with Undo showing it again.
    #[test]
    fn hiding_a_built_in_can_be_undone() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let mut e = editor(&ctx, dir.path());
        e.hide_preset("style:rectangle".into(), "Rectangle");
        assert!(
            e.quick_controls
                .preset_records()
                .hidden
                .contains(&"style:rectangle".to_owned())
        );
        let (_, action) = e.notice.action.clone().unwrap();
        e.run_fix(action, &ctx);
        assert!(e.quick_controls.preset_records().hidden.is_empty());
    }
}
