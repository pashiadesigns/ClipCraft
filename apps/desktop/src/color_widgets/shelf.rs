//! Named palettes and Recent colors in every color picker (UX-010, approved
//! bi-proposal 3). The chosen palette is shared by all pickers and kept
//! between sessions; the built-in ClipDesigner palette can't be edited.
use super::*;
use capture_core::palettes::{BUILT_IN_COLORS, Edit, Palettes, Store};

#[derive(Clone, Copy, PartialEq)]
enum Menu {
    Swatch([u8; 4]),
    Palette,
    /// The palette list. It is the picker's own menu, not a ComboBox, so
    /// choosing a palette keeps the color picker open (BI6).
    Choose,
}
/// A small button with a drawn glyph, so it never depends on a font having
/// the character (BI6: the ⋯ glyph drew as an empty box).
fn glyph_button(ui: &mut egui::Ui, size: egui::Vec2, dots: bool) -> egui::Response {
    let r = ui.add_sized(size, egui::Button::new(""));
    let ink = ui.visuals().text_color();
    let c = r.rect.center();
    if dots {
        for dx in [-5., 0., 5.] {
            ui.painter().circle_filled(c + egui::vec2(dx, 0.), 1.8, ink);
        }
    } else {
        let x = r.rect.right() - 12.;
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x - 4., c.y - 2.),
                egui::pos2(x + 4., c.y - 2.),
                egui::pos2(x, c.y + 3.),
            ],
            ink,
            egui::Stroke::NONE,
        ));
    }
    r
}
#[derive(Clone, Copy, PartialEq)]
enum Naming {
    New,
    Rename,
}
#[derive(Clone, Default)]
struct Shelf {
    store: Option<Store>,
    palettes: Palettes,
    error: Option<String>,
    context: Option<(Menu, egui::Pos2)>,
    context_rect: Option<egui::Rect>,
    naming: Option<(Naming, String)>,
}
fn key() -> egui::Id {
    egui::Id::unique("my-colors")
}
fn image_request() -> egui::Id {
    egui::Id::unique("palette-from-image")
}
impl Shelf {
    fn load() -> Self {
        // Headless UI tests never read or write the workstation's preferences.
        #[cfg(test)]
        {
            Self::default()
        }
        #[cfg(not(test))]
        {
            let root = std::env::var_os("CLIPDESIGNER_PREFERENCES_DIR")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("LOCALAPPDATA")
                        .map(|p| std::path::PathBuf::from(p).join("ClipDesigner/preferences-v1"))
                });
            let Some(root) = root else {
                return Self {
                    error: Some("Local color storage unavailable".into()),
                    ..Self::default()
                };
            };
            match Store::open(&root).and_then(|store| store.read().map(|p| (store, p))) {
                Ok((store, palettes)) => Self {
                    store: Some(store),
                    palettes,
                    ..Default::default()
                },
                Err(e) => Self {
                    error: Some(e.to_string()),
                    ..Self::default()
                },
            }
        }
    }
    fn edit(&mut self, edit: Edit) {
        match &self.store {
            Some(store) => match store.edit(edit) {
                Ok(p) => {
                    self.palettes = p;
                    self.error = None;
                }
                Err(e) => self.error = Some(e.to_string()),
            },
            None if self.error.is_none() => {
                self.error = Some("Palettes can't be saved on this PC.".into())
            }
            None => {}
        }
    }
}
/// The palettes on disk, for kits; `None` in tests and when there's no
/// preferences folder.
pub(crate) fn palette_store() -> Option<Store> {
    #[cfg(test)]
    {
        None
    }
    #[cfg(not(test))]
    {
        Shelf::load().store
    }
}
/// Every color picker rereads its palettes, after a kit added some.
pub(crate) fn reload(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Shelf>(key()));
}
fn with_shelf<R>(ctx: &egui::Context, f: impl FnOnce(&mut Shelf) -> R) -> R {
    let mut shelf = ctx
        .data_mut(|d| d.get_temp::<Shelf>(key()))
        .unwrap_or_else(Shelf::load);
    let r = f(&mut shelf);
    ctx.data_mut(|d| d.insert_temp(key(), shelf));
    r
}
pub(super) fn context_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<Shelf>(key())
            .is_some_and(|s| s.context.is_some())
    })
}
pub(super) fn context_contains(ctx: &egui::Context, pos: egui::Pos2) -> bool {
    ctx.data(|d| {
        d.get_temp::<Shelf>(key())
            .is_some_and(|s| s.context.is_some() && s.context_rect.is_some_and(|r| r.contains(pos)))
    })
}
pub(super) fn close_context(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        if let Some(mut s) = d.get_temp::<Shelf>(key()) {
            s.context = None;
            s.context_rect = None;
            d.insert_temp(key(), s);
        }
    });
}
/// Remembers a color the user chose, for the Recent row.
pub(super) fn push_recent(ctx: &egui::Context, color: [u8; 4]) {
    with_shelf(ctx, |s| {
        if s.store.is_some() {
            s.edit(Edit::Recent(color));
        } else {
            s.palettes.recent.retain(|c| *c != color);
            s.palettes.recent.insert(0, color);
            s.palettes.recent.truncate(8);
        }
    });
}
/// "New from image colors" asks the editor for the open image's colors.
pub(crate) fn take_image_request(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(image_request()).unwrap_or(false))
}
pub(crate) fn create_palette(ctx: &egui::Context, name: &str, colors: Vec<[u8; 4]>) {
    with_shelf(ctx, |s| {
        s.edit(Edit::Create {
            name: name.into(),
            colors,
        })
    });
}
fn swatch_label(palettes: &Palettes, color: [u8; 4]) -> String {
    if palettes.editable() {
        hex(color)
    } else {
        BUILT_IN_COLORS
            .iter()
            .find(|(_, c)| *c == color)
            .map_or_else(|| hex(color), |(n, _)| (*n).into())
    }
}
/// Palette chooser, its swatches and the palette menu, at the top of the picker.
pub(super) fn palette(ui: &mut egui::Ui, draft: &mut ColorDraft, opaque: bool) {
    let mut shelf = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Shelf>(key()))
        .unwrap_or_else(Shelf::load);
    let usable = parse_hex(&draft.hex).is_some_and(|c| !opaque || c[3] == 255);
    ui.horizontal(|ui| {
        let width = ui.available_width() - 36.;
        let choose = glyph_button(ui, egui::vec2(width, 24.), false);
        ui.painter().with_clip_rect(choose.rect.shrink(4.)).text(
            choose.rect.left_center() + egui::vec2(8., 0.),
            egui::Align2::LEFT_CENTER,
            &shelf.palettes.selected,
            egui::TextStyle::Button.resolve(ui.style()),
            ui.visuals().text_color(),
        );
        let choose = choose
            .tip("Palette: the colors shown in every color picker. Your choice is remembered.");
        if choose.clicked() {
            shelf.context = if shelf.context.is_some_and(|(m, _)| m == Menu::Choose) {
                None
            } else {
                Some((Menu::Choose, choose.rect.left_bottom()))
            };
            shelf.naming = None;
        }
        let more = glyph_button(ui, egui::vec2(28., 24.), true).tip(
            "Palette menu: new, rename, duplicate, from image colors, import, export, delete.",
        );
        if more.clicked() {
            shelf.context = Some((Menu::Palette, more.rect.left_bottom()));
            shelf.naming = None;
        }
    });
    if let Some((naming, name)) = shelf.naming.as_mut() {
        let naming = *naming;
        let mut done = None;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("Palette name")
                    .desired_width(ui.available_width() - 120.),
            );
            if !field.has_focus() && !field.lost_focus() {
                field.request_focus();
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .button(if naming == Naming::New {
                    "Create"
                } else {
                    "Rename"
                })
                .clicked()
                || enter
            {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
        if let Some(apply) = done {
            let name = name.clone();
            shelf.naming = None;
            if apply {
                shelf.edit(match naming {
                    Naming::New => Edit::Create {
                        name,
                        colors: vec![],
                    },
                    Naming::Rename => Edit::Rename(name),
                });
            }
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
        for color in shelf.palettes.colors() {
            if opaque && color[3] != 255 {
                continue;
            }
            let label = swatch_label(&shelf.palettes, color);
            let r = color_swatch(ui, color, &label, draft.color == color);
            if r.clicked() {
                draft.set_color(color);
                shelf.context = None;
            }
            if r.secondary_clicked() && shelf.palettes.editable() {
                shelf.context = Some((
                    Menu::Swatch(color),
                    r.interact_pointer_pos().unwrap_or(r.rect.right_bottom()),
                ));
            }
        }
        if !opaque && color_swatch(ui, [0; 4], "Transparent", draft.color[3] == 0).clicked() {
            draft.set_color([0; 4]);
        }
        if shelf.palettes.editable()
            && ui
                .add_enabled(
                    usable,
                    egui::Button::new("+").min_size(egui::vec2(22.0, 22.0)),
                )
                .tip("Add the current color to this palette")
                .clicked()
        {
            shelf.edit(Edit::Add(draft.color));
        }
    });
    if let Some((menu, anchor)) = shelf.context {
        let mut open = true;
        let mut action = None;
        let popup = egui::Popup::new(
            ui.scope_id().with("palette-actions"),
            ui.ctx().clone(),
            anchor,
            ui.layer_id(),
        )
        .open_bool(&mut open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .show(|ui| match menu {
            Menu::Choose => {
                ui.set_min_width(160.);
                for name in shelf.palettes.names() {
                    if ui
                        .selectable_label(name == shelf.palettes.selected, &name)
                        .clicked()
                    {
                        action = Some(Edit::Select(name));
                    }
                }
            }
            Menu::Swatch(old) => {
                if ui
                    .add_enabled(usable, egui::Button::new("Replace with current color"))
                    .tip("Replace this swatch with the current color and transparency")
                    .clicked()
                {
                    action = Some(Edit::Replace {
                        old,
                        new: draft.color,
                    });
                }
                if ui
                    .button("Move left")
                    .tip("Move this swatch one place left")
                    .clicked()
                {
                    action = Some(Edit::Move {
                        color: old,
                        delta: -1,
                    });
                }
                if ui
                    .button("Move right")
                    .tip("Move this swatch one place right")
                    .clicked()
                {
                    action = Some(Edit::Move {
                        color: old,
                        delta: 1,
                    });
                }
                if ui
                    .button("Remove from palette")
                    .tip("Remove this swatch")
                    .clicked()
                {
                    action = Some(Edit::Remove(old));
                }
            }
            Menu::Palette => {
                let editable = shelf.palettes.editable();
                if ui
                    .button("New palette…")
                    .tip("Make an empty palette")
                    .clicked()
                {
                    shelf.naming = Some((Naming::New, "New palette".into()));
                    ui.close();
                }
                if ui
                    .add_enabled(editable, egui::Button::new("Rename…"))
                    .tip("Rename this palette")
                    .clicked()
                {
                    shelf.naming = Some((Naming::Rename, shelf.palettes.selected.clone()));
                    ui.close();
                }
                if ui
                    .button("Duplicate")
                    .tip("Copy this palette to edit it")
                    .clicked()
                {
                    action = Some(Edit::Duplicate);
                }
                if ui
                    .button("New from image colors")
                    .tip("Make a palette from the main colors in the open image")
                    .clicked()
                {
                    ui.ctx().data_mut(|d| d.insert_temp(image_request(), true));
                    ui.close();
                }
                if ui
                    .button("Import (.gpl, .ase)…")
                    .tip("Add a GIMP or Adobe palette file")
                    .clicked()
                {
                    #[cfg(not(test))]
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Palettes", &["gpl", "ase"])
                        .pick_file()
                    {
                        match capture_core::palettes::import(&path) {
                            Ok(p) => {
                                action = Some(Edit::Create {
                                    name: p.name,
                                    colors: p.colors,
                                })
                            }
                            Err(e) => shelf.error = Some(e.to_string()),
                        }
                    }
                    ui.close();
                }
                if ui
                    .button("Export…")
                    .tip("Save this palette as a GIMP (.gpl) or Adobe (.ase) file")
                    .clicked()
                {
                    #[cfg(not(test))]
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("{}.gpl", shelf.palettes.selected))
                        .add_filter("GIMP palette", &["gpl"])
                        .add_filter("Adobe swatch exchange", &["ase"])
                        .save_file()
                    {
                        let palette = capture_core::palettes::Palette {
                            name: shelf.palettes.selected.clone(),
                            colors: shelf.palettes.colors(),
                        };
                        if let Err(e) = capture_core::palettes::export(&palette, &path) {
                            shelf.error = Some(e.to_string());
                        }
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        editable,
                        egui::Button::new(
                            egui::RichText::new("Delete palette")
                                .color(egui::Color32::from_rgb(228, 135, 127)),
                        ),
                    )
                    .tip("Delete this palette. The ClipDesigner palette is selected instead.")
                    .clicked()
                {
                    action = Some(Edit::Delete);
                }
            }
        });
        shelf.context_rect = popup.map(|m| m.response.rect);
        if let Some(edit) = action.take() {
            shelf.edit(edit);
            open = false;
        }
        if !open || shelf.naming.is_some() {
            shelf.context = None;
            shelf.context_rect = None;
        }
    }
    if let Some(error) = &shelf.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
    ui.ctx().data_mut(|d| d.insert_temp(key(), shelf));
}
/// The Recent row at the bottom of the picker: the last colors chosen anywhere.
pub(super) fn show(ui: &mut egui::Ui, draft: &mut ColorDraft, opaque: bool) {
    let recent: Vec<[u8; 4]> = with_shelf(ui.ctx(), |s| s.palettes.recent.clone());
    if recent.is_empty() {
        return;
    }
    ui.separator();
    ui.label(egui::RichText::new("Recent").strong());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
        for color in recent {
            if opaque && color[3] != 255 {
                continue;
            }
            if color_swatch(
                ui,
                color,
                &format!("recent {}", hex(color)),
                draft.color == color,
            )
            .clicked()
            {
                draft.set_color(color);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use capture_core::palettes::BUILT_IN;
    use egui::{Event, Modifiers, PointerButton};
    fn frame(
        ctx: &egui::Context,
        draft: &mut ColorDraft,
        events: Vec<Event>,
    ) -> crate::property_layout::tests::PaintedOutput {
        crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(420.0, 420.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                palette(ui, draft, false);
                show(ui, draft, false);
            },
        ))
    }
    fn click(ctx: &egui::Context, draft: &mut ColorDraft, p: egui::Pos2, button: PointerButton) {
        for pressed in [true, false] {
            frame(
                ctx,
                draft,
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn palette_add_replace_remove_and_recent_use_exact_rgba() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        store
            .edit(Edit::Create {
                name: "Acme brand".into(),
                colors: vec![[13, 57, 91, 180]],
            })
            .unwrap();
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.data_mut(|d| {
            d.insert_temp(
                key(),
                Shelf {
                    palettes: store.read().unwrap(),
                    store: Some(store.clone()),
                    ..Default::default()
                },
            )
        });
        let mut draft = ColorDraft::new([17, 85, 201, 0]);
        frame(&ctx, &mut draft, vec![]);
        let out = frame(&ctx, &mut draft, vec![]);
        let plus = super::super::tests::text_rect(&out.shapes, "+")
            .unwrap()
            .center();
        click(&ctx, &mut draft, plus, PointerButton::Primary);
        assert_eq!(
            store.read().unwrap().colors(),
            [[13, 57, 91, 180], [17, 85, 201, 0]]
        );
        // Right-click a swatch: Replace keeps the order and exact alpha.
        let old = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique((
                    "preset-swatch",
                    hex([13, 57, 91, 180]),
                )))
            })
            .unwrap();
        draft.set_color([1, 2, 3, 4]);
        click(&ctx, &mut draft, old.center(), PointerButton::Secondary);
        frame(&ctx, &mut draft, vec![]);
        let out = frame(&ctx, &mut draft, vec![]);
        assert!(context_open(&ctx));
        let replace =
            super::super::tests::text_rect(&out.shapes, "Replace with current color").unwrap();
        click(&ctx, &mut draft, replace.center(), PointerButton::Primary);
        assert_eq!(
            store.read().unwrap().colors(),
            [[1, 2, 3, 4], [17, 85, 201, 0]]
        );
        assert!(!context_open(&ctx));
        push_recent(&ctx, [9, 9, 9, 255]);
        let out = frame(&ctx, &mut draft, vec![]);
        assert!(super::super::tests::text_rect(&out.shapes, "Recent").is_some());
        assert_eq!(store.read().unwrap().recent, [[9, 9, 9, 255]]);
    }
    #[test]
    fn built_in_palette_has_named_swatches_and_no_add_button() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut draft = ColorDraft::new([255; 4]);
        frame(&ctx, &mut draft, vec![]);
        let out = frame(&ctx, &mut draft, vec![]);
        for name in ["White", "Red", "Yellow", "Transparent"] {
            assert!(
                ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("preset-swatch", name))))
                    .is_some(),
                "{name}"
            );
        }
        assert!(super::super::tests::text_rect(&out.shapes, "+").is_none());
        assert!(super::super::tests::text_rect(&out.shapes, BUILT_IN).is_some());
    }
}
