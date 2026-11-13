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

/// Width of the preset menu: compact, like the app's other short menus (BY10:
/// "way too large").
pub(crate) const WIDTH: f32 = 176.;
const ROW: f32 = 24.;

const DANGER: egui::Color32 = egui::Color32::from_rgb(142, 58, 52);

/// The red Delete fill, lighter while hovered and darker while pressed, so
/// the click visibly "goes through" like Keep (BZ2).
pub(crate) fn danger_fill(r: &egui::Response) -> egui::Color32 {
    if r.is_pointer_button_down_on() {
        egui::Color32::from_rgb(116, 40, 36)
    } else if r.hovered() {
        egui::Color32::from_rgb(190, 72, 64)
    } else {
        DANGER
    }
}

/// A red Delete button with hover and pressed states.
fn danger_button(ui: &mut egui::Ui, label: &str, size: egui::Vec2) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let p = ui.painter();
    p.rect_filled(rect, 4., danger_fill(&r));
    if r.hovered() {
        p.rect_stroke(
            rect,
            4.,
            egui::Stroke::new(1., egui::Color32::from_rgb(255, 196, 190)),
            egui::StrokeKind::Inside,
        );
    }
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.),
        egui::Color32::WHITE,
    );
    r
}

/// An × drawn as two strokes, exactly centered on `center` (a text glyph
/// sits off center in its circle, BZ2).
pub(crate) fn paint_cross(p: &egui::Painter, center: egui::Pos2, half: f32, color: egui::Color32) {
    let s = egui::Stroke::new(1.6, color);
    p.line_segment(
        [
            center + egui::vec2(-half, -half),
            center + egui::vec2(half, half),
        ],
        s,
    );
    p.line_segment(
        [
            center + egui::vec2(-half, half),
            center + egui::vec2(half, -half),
        ],
        s,
    );
}

/// One compact menu row.
fn row(ui: &mut egui::Ui, label: &str, enabled: bool, danger: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(
        egui::vec2(WIDTH, ROW),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("preset-menu-row", label)), rect));
    let p = ui.painter();
    let hot = enabled && r.hovered();
    if hot {
        p.rect_filled(
            rect,
            4.,
            if danger {
                egui::Color32::from_rgb(142, 58, 52)
            } else {
                crate::shell::ACCENT
            },
        );
    }
    let text = if !enabled {
        egui::Color32::from_gray(124)
    } else if danger && !hot {
        crate::guided::RED
    } else {
        egui::Color32::from_rgb(232, 238, 240)
    };
    p.text(
        rect.left_center() + egui::vec2(10., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.),
        text,
    );
    r
}

/// The preset menu on `response` (a tile, card or row). It stays open while
/// you rename or answer Delete's question; any other choice closes it.
pub(crate) fn context_menu(response: &egui::Response, m: &Menu) -> Option<Choice> {
    let key = response.id.with("preset-menu-stage");
    // A fresh right-click always starts at the menu, never at an old question.
    if response.secondary_clicked() {
        response.ctx.data_mut(|d| d.insert_temp(key, Stage::Menu));
    }
    let mut chosen = None;
    egui::Popup::context_menu(response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(WIDTH)
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("preset-menu-rect"), ui.max_rect()));
            let stage = ui
                .ctx()
                .data(|d| d.get_temp::<Stage>(key))
                .unwrap_or(Stage::Menu);
            let mut next = stage.clone();
            match stage {
                Stage::Confirm => {
                    // Delete asks in the same open menu.
                    ui.add_space(4.);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("Delete \u{201c}{}\u{201d}?", m.name))
                                .strong()
                                .size(13.),
                        )
                        .wrap()
                        .selectable(false),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("You can undo this.")
                                .size(11.5)
                                .color(egui::Color32::from_gray(150)),
                        )
                        .selectable(false),
                    );
                    ui.add_space(6.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.;
                        if ui
                            .add(crate::controls::dialog_button("Keep", false))
                            .clicked()
                        {
                            next = Stage::Menu;
                        }
                        let delete = danger_button(
                            ui,
                            "Delete",
                            egui::vec2(82., crate::controls::DIALOG_ACTION_HEIGHT),
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::unique("preset-menu-confirm"), delete.rect)
                        });
                        if delete.clicked() {
                            chosen = Some(Choice::Delete);
                            next = Stage::Menu;
                            ui.close();
                        }
                    });
                    ui.add_space(4.);
                }
                Stage::Rename(mut text) => {
                    // Rename edits the name in place.
                    ui.add_space(4.);
                    let field = ui.add(
                        crate::unicode_input::TextEdit::singleline(&mut text)
                            .id(key.with("rename"))
                            .desired_width(WIDTH - 8.)
                            .char_limit(80),
                    );
                    field.request_focus();
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(6.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.;
                        if ui
                            .add(crate::controls::dialog_button("Cancel", false))
                            .clicked()
                        {
                            next = Stage::Menu;
                        } else if ui
                            .add(crate::controls::dialog_button("Rename", true))
                            .clicked()
                            || enter
                        {
                            if !text.trim().is_empty() {
                                chosen = Some(Choice::Rename(text.trim().to_owned()));
                            }
                            next = Stage::Menu;
                            ui.close();
                        } else {
                            next = Stage::Rename(text.clone());
                        }
                    });
                    ui.add_space(4.);
                }
                Stage::Menu => {
                    let mut pick = |ui: &mut egui::Ui, c: Choice| {
                        chosen = Some(c);
                        ui.close();
                    };
                    if m.apply && row(ui, "Apply", true, false).clicked() {
                        pick(ui, Choice::Apply);
                    }
                    if let Some((label, enabled)) = m.update
                        && row(ui, label, enabled, false).clicked()
                    {
                        pick(ui, Choice::Update);
                    }
                    if m.rename && row(ui, "Rename", true, false).clicked() {
                        next = Stage::Rename(m.name.to_owned());
                    }
                    if m.duplicate && row(ui, "Duplicate", true, false).clicked() {
                        pick(ui, Choice::Duplicate);
                    }
                    if let Some(on) = m.favorite {
                        let label = if on { "Unfavorite" } else { "Favorite" };
                        if row(ui, label, true, false).clicked() {
                            pick(ui, Choice::Favorite(!on));
                        }
                    }
                    if m.kit.is_some() || m.hide || m.delete {
                        ui.add_space(3.);
                        ui.separator();
                        ui.add_space(3.);
                    }
                    if let Some(kit) = m.kit {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("From kit: {kit}"))
                                    .size(11.5)
                                    .color(egui::Color32::from_gray(150)),
                            )
                            .truncate()
                            .selectable(false),
                        );
                    }
                    if m.hide && row(ui, "Hide", true, false).clicked() {
                        pick(ui, Choice::Hide);
                    }
                    if m.delete && row(ui, "Delete\u{2026}", true, true).clicked() {
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
        p.rect_filled(yes, 3., danger_fill(&yes_r));
        p.rect_filled(
            no,
            3.,
            if no_r.hovered() {
                egui::Color32::from_rgb(96, 108, 113)
            } else {
                egui::Color32::from_rgb(74, 84, 88)
            },
        );
        for (r, rect) in [(&yes_r, yes), (&no_r, no)] {
            if r.hovered() {
                p.rect_stroke(
                    rect,
                    3.,
                    egui::Stroke::new(1., egui::Color32::from_gray(225)),
                    egui::StrokeKind::Inside,
                );
            }
        }
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
    paint_cross(p, x.center(), 3.5, egui::Color32::WHITE);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// BY10: the menu is compact, Delete… asks inside the same open menu,
    /// Delete answers with Choice::Delete, and a new right-click starts at
    /// the menu again.
    #[test]
    fn delete_asks_inside_the_open_compact_menu() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
        let tile = egui::Rect::from_min_size(egui::pos2(40., 40.), egui::vec2(82., 52.));
        let menu = Menu {
            apply: true,
            update: Some(("Update from selection", true)),
            rename: true,
            duplicate: true,
            delete: true,
            name: "Brand box",
            ..Default::default()
        };
        let frame = |events: Vec<egui::Event>| {
            let mut chosen = None;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    let r = ui.interact(tile, egui::Id::unique("tile"), egui::Sense::click());
                    chosen = context_menu(&r, &menu);
                },
            );
            out.textures_delta.clear();
            chosen
        };
        let click = |at: egui::Pos2, button| {
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: at,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let row = |label: &str| -> egui::Rect {
            ctx.data(|d| d.get_temp(egui::Id::unique(("preset-menu-row", label))))
                .unwrap_or_else(|| panic!("{label} row"))
        };
        frame(vec![]);
        frame(click(tile.center(), egui::PointerButton::Secondary));
        frame(vec![]);
        assert!(egui::Popup::is_any_open(&ctx), "the menu opened");
        let area: egui::Rect = ctx
            .data(|d| d.get_temp(egui::Id::unique("preset-menu-rect")))
            .unwrap();
        assert!(area.width() <= WIDTH + 1., "compact: {area:?}");
        assert!(row("Apply").height() <= ROW + 0.1);
        let delete = row("Delete\u{2026}").center();
        assert_eq!(frame(click(delete, egui::PointerButton::Primary)), None);
        frame(vec![]);
        assert!(
            egui::Popup::is_any_open(&ctx),
            "Delete\u{2026} keeps the menu open and asks there"
        );
        let confirm: egui::Rect = ctx
            .data(|d| d.get_temp(egui::Id::unique("preset-menu-confirm")))
            .expect("the question shows in the same menu");
        assert_eq!(
            frame(click(confirm.center(), egui::PointerButton::Primary)),
            Some(Choice::Delete)
        );
        frame(vec![]);
        assert!(!egui::Popup::is_any_open(&ctx), "Delete closes the menu");
        // Ask again, then dismiss by clicking elsewhere: the next right-click
        // starts at the menu, not at the old question.
        frame(click(tile.center(), egui::PointerButton::Secondary));
        frame(vec![]);
        frame(click(
            row("Delete\u{2026}").center(),
            egui::PointerButton::Primary,
        ));
        frame(click(egui::pos2(700., 500.), egui::PointerButton::Primary));
        frame(vec![]);
        assert!(!egui::Popup::is_any_open(&ctx));
        ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::unique(("preset-menu-row", "Apply"))));
        frame(click(tile.center(), egui::PointerButton::Secondary));
        frame(vec![]);
        row("Apply");
    }
}
