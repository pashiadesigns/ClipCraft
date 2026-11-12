//! Removing presets (approved mockup, "approve all of it"): one right-click
//! menu on every preset in every gallery, an × on saved tiles, Hide for
//! built-ins and kit items, and Undo in the message bar after a delete.
use crate::egui;

/// What the menu offers for one preset. Only what applies is shown.
#[derive(Default)]
pub(crate) struct Menu<'a> {
    pub apply: bool,
    /// "Update from selection" (or the gallery's own wording) and whether it
    /// can be used now.
    pub update: Option<(&'a str, bool)>,
    pub rename: bool,
    pub duplicate: bool,
    /// Favorite: Some(current) where the gallery has favorites.
    pub favorite: Option<bool>,
    /// Built-ins and items from a team kit can be hidden.
    pub hide: bool,
    /// Built-ins can't be deleted.
    pub delete: bool,
    /// The kit an item came from.
    pub kit: Option<&'a str>,
    pub name: &'a str,
}

/// What was chosen.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Choice {
    Apply,
    Update,
    Rename(String),
    Duplicate,
    Favorite(bool),
    Hide,
    Delete,
}

#[derive(Clone)]
enum Stage {
    Menu,
    Confirm,
    Rename(String),
}

/// The preset menu on `response` (a tile, card or row).
pub(crate) fn context_menu(response: &egui::Response, m: &Menu) -> Option<Choice> {
    let key = response.id.with("preset-menu-stage");
    let mut chosen = None;
    response.context_menu(|ui| {
        ui.set_min_width(200.);
        let stage = ui
            .ctx()
            .data(|d| d.get_temp::<Stage>(key))
            .unwrap_or(Stage::Menu);
        let mut next = stage.clone();
        match stage {
            Stage::Confirm => {
                // Delete asks inside the menu.
                ui.label(
                    egui::RichText::new(format!("Delete \u{201c}{}\u{201d}?", m.name)).strong(),
                );
                ui.weak("You can undo this.");
                ui.horizontal(|ui| {
                    if ui.button("Keep").clicked() {
                        next = Stage::Menu;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Delete").color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(142, 58, 52)),
                        )
                        .clicked()
                    {
                        chosen = Some(Choice::Delete);
                        next = Stage::Menu;
                        ui.close();
                    }
                });
            }
            Stage::Rename(mut text) => {
                // Rename edits the name in place.
                let field = ui.add(
                    crate::unicode_input::TextEdit::singleline(&mut text)
                        .id(key.with("rename"))
                        .desired_width(190.)
                        .char_limit(80),
                );
                field.request_focus();
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        next = Stage::Menu;
                    } else if ui.button("Rename").clicked() || enter {
                        if !text.trim().is_empty() {
                            chosen = Some(Choice::Rename(text.trim().to_owned()));
                        }
                        next = Stage::Menu;
                        ui.close();
                    } else {
                        next = Stage::Rename(text.clone());
                    }
                });
            }
            Stage::Menu => {
                let item = |ui: &mut egui::Ui, label: &str, key: &str, enabled: bool| {
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.horizontal(|ui| {
                            let r = crate::controls::menu_action(ui, label);
                            if !key.is_empty() {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.weak(key);
                                    },
                                );
                            }
                            r
                        })
                        .inner
                    })
                    .inner
                    .clicked()
                };
                if m.apply && item(ui, "Apply", "", true) {
                    chosen = Some(Choice::Apply);
                    ui.close();
                }
                if let Some((label, enabled)) = m.update
                    && item(ui, label, "", enabled)
                {
                    chosen = Some(Choice::Update);
                    ui.close();
                }
                if m.rename && item(ui, "Rename", "F2", true) {
                    next = Stage::Rename(m.name.to_owned());
                }
                if m.duplicate && item(ui, "Duplicate", "Ctrl+D", true) {
                    chosen = Some(Choice::Duplicate);
                    ui.close();
                }
                if let Some(on) = m.favorite
                    && item(
                        ui,
                        if on {
                            "Remove from favorites"
                        } else {
                            "Favorite"
                        },
                        "",
                        true,
                    )
                {
                    chosen = Some(Choice::Favorite(!on));
                    ui.close();
                }
                if m.kit.is_some() || m.hide || m.delete {
                    ui.separator();
                }
                if let Some(kit) = m.kit {
                    ui.add_enabled(false, egui::Label::new(format!("From kit: {kit}")));
                }
                if m.hide && item(ui, "Hide", "", true) {
                    chosen = Some(Choice::Hide);
                    ui.close();
                }
                if m.delete
                    && ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Delete\u{2026}").color(crate::guided::RED),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    next = Stage::Confirm;
                }
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(key, next));
    });
    chosen
}

/// The × on a saved tile or card while it's hovered. Clicking it asks
/// inside the tile; returns true when Delete is chosen.
pub(crate) fn hover_delete(ui: &mut egui::Ui, rect: egui::Rect, id: egui::Id, name: &str) -> bool {
    let asking_key = id.with("hover-delete-asking");
    let asking = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(asking_key))
        .unwrap_or(false);
    let mut delete = false;
    if asking {
        let p = ui.painter();
        p.rect_filled(
            rect,
            6.,
            egui::Color32::from_rgba_unmultiplied(74, 47, 45, 235),
        );
        p.rect_stroke(
            rect,
            6.,
            egui::Stroke::new(1., crate::guided::RED),
            egui::StrokeKind::Inside,
        );
        let mid = rect.center();
        p.text(
            mid - egui::vec2(0., 9.),
            egui::Align2::CENTER_CENTER,
            "Delete?",
            egui::FontId::proportional(11.5),
            egui::Color32::from_rgb(255, 217, 213),
        );
        let w = (rect.width() / 2. - 6.).clamp(20., 46.);
        let yes =
            egui::Rect::from_center_size(mid + egui::vec2(-w / 2. - 2., 9.), egui::vec2(w, 16.));
        let no =
            egui::Rect::from_center_size(mid + egui::vec2(w / 2. + 2., 9.), egui::vec2(w, 16.));
        let yes_r = ui.interact(yes, id.with("hover-delete-yes"), egui::Sense::click());
        let no_r = ui.interact(no, id.with("hover-delete-no"), egui::Sense::click());
        let p = ui.painter();
        p.rect_filled(yes, 3., egui::Color32::from_rgb(142, 58, 52));
        p.rect_filled(no, 3., egui::Color32::from_rgb(74, 84, 88));
        p.text(
            yes.center(),
            egui::Align2::CENTER_CENTER,
            "Delete",
            egui::FontId::proportional(10.),
            egui::Color32::WHITE,
        );
        p.text(
            no.center(),
            egui::Align2::CENTER_CENTER,
            "Keep",
            egui::FontId::proportional(10.),
            egui::Color32::WHITE,
        );
        if yes_r.clicked() {
            delete = true;
            ui.ctx().data_mut(|d| d.insert_temp(asking_key, false));
        } else if no_r.clicked()
            || (ui.input(|i| i.pointer.any_pressed()) && !ui.rect_contains_pointer(rect))
        {
            ui.ctx().data_mut(|d| d.insert_temp(asking_key, false));
        }
        return delete;
    }
    if !ui.rect_contains_pointer(rect.expand(2.)) {
        return false;
    }
    let x = egui::Rect::from_center_size(
        rect.right_top() + egui::vec2(-10., 10.),
        egui::vec2(18., 18.),
    );
    use crate::tips::Tip as _;
    let r = ui
        .interact(x, id.with("hover-delete-x"), egui::Sense::click())
        .tip(format!("Delete \u{201c}{name}\u{201d}"));
    let p = ui.painter();
    p.circle_filled(
        x.center(),
        9.,
        if r.hovered() {
            egui::Color32::from_rgb(142, 58, 52)
        } else {
            egui::Color32::from_black_alpha(150)
        },
    );
    p.text(
        x.center(),
        egui::Align2::CENTER_CENTER,
        "\u{2715}",
        egui::FontId::proportional(10.),
        egui::Color32::WHITE,
    );
    if r.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(asking_key, true));
    }
    false
}

/// "Name (2)", "Name (3)"... not in `taken`.
pub(crate) fn copy_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    (2..)
        .map(|n| format!("{} ({n})", name.trim()))
        .find(|c| !taken(c))
        .unwrap()
}
