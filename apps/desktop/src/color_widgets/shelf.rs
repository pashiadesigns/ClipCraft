use super::*;
#[derive(Clone, Default)]
struct Shelf {
    store: Option<capture_core::saved_colors::Store>,
    colors: Vec<[u8; 4]>,
    error: Option<String>,
    context: Option<([u8; 4], egui::Pos2)>,
    context_rect: Option<egui::Rect>,
}
fn key() -> egui::Id {
    egui::Id::unique("my-colors")
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
            let root = std::env::var_os("CLIPCRAFT_PREFERENCES_DIR")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("LOCALAPPDATA")
                        .map(|p| std::path::PathBuf::from(p).join("ClipCraft/preferences-v1"))
                });
            let Some(root) = root else {
                return Self {
                    error: Some("Local color storage unavailable".into()),
                    ..Self::default()
                };
            };
            match capture_core::saved_colors::Store::open(&root)
                .and_then(|store| store.read().map(|colors| (store, colors)))
            {
                Ok((store, colors)) => Self {
                    store: Some(store),
                    colors,
                    error: None,
                    ..Default::default()
                },
                Err(e) => Self {
                    error: Some(e.to_string()),
                    ..Self::default()
                },
            }
        }
    }
    fn update(&mut self, color: [u8; 4], add: bool) {
        if let Some(store) = &self.store {
            match store.set(color, add) {
                Ok(colors) => {
                    self.colors = colors;
                    self.error = None;
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        }
    }
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
pub(super) fn show(ui: &mut egui::Ui, draft: &mut ColorDraft, opaque: bool) {
    let mut shelf = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Shelf>(key()))
        .unwrap_or_else(Shelf::load);
    ui.separator();
    ui.horizontal(|ui| {
        ui.strong("My Colors");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    shelf.store.is_some()
                        && parse_hex(&draft.hex).is_some_and(|c| !opaque || c[3] == 255),
                    egui::Button::new("+").min_size(egui::vec2(24.0, 24.0)),
                )
                .on_hover_text("Save the current color")
                .clicked()
            {
                shelf.update(draft.color, true);
            }
        });
    });
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("saved-colors")
        .max_height(58.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                for color in &shelf.colors {
                    if opaque && color[3] != 255 {
                        continue;
                    }
                    let r = color_swatch(ui, *color, &hex(*color), draft.color == *color);
                    if r.clicked() {
                        draft.set_color(*color);
                        shelf.context = None;
                    }
                    if r.secondary_clicked() {
                        shelf.context = Some((
                            *color,
                            r.interact_pointer_pos().unwrap_or(r.rect.right_bottom()),
                        ));
                    }
                }
            });
        });
    if let Some((old, anchor)) = shelf.context {
        let mut open = true;
        let mut update = false;
        let menu = egui::Popup::new(
            ui.scope_id().with("saved-color-actions"),
            ui.ctx().clone(),
            anchor,
            ui.layer_id(),
        )
        .open_bool(&mut open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .show(|ui| {
            if ui
                .add_enabled(
                    parse_hex(&draft.hex).is_some_and(|c| !opaque || c[3] == 255),
                    egui::Button::new("Update from Current Color"),
                )
                .on_hover_text("Replace this swatch with the current color and transparency")
                .clicked()
            {
                update = true;
                ui.close();
            }
            if ui.button("Remove from My Colors").clicked() {
                remove = Some(old);
                ui.close();
            }
        });
        shelf.context_rect = menu.map(|m| m.response.rect);
        if update && let Some(store) = &shelf.store {
            match store.replace(old, draft.color) {
                Ok(colors) => {
                    shelf.colors = colors;
                    shelf.error = None;
                }
                Err(e) => shelf.error = Some(e.to_string()),
            }
        }
        if !open || update || remove.is_some() {
            shelf.context = None;
            shelf.context_rect = None;
        }
    }
    if let Some(color) = remove {
        shelf.update(color, false);
    }
    if let Some(error) = &shelf.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
    ui.ctx().data_mut(|d| d.insert_temp(key(), shelf));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_color_context_updates_alpha_and_removes_without_closing_picker() {
        use crate::property_layout::tests::PaintedOutput;
        use egui::{Event, Modifiers, PointerButton};
        let temp = tempfile::tempdir().unwrap();
        let store = capture_core::saved_colors::Store::open(temp.path()).unwrap();
        let old = [13, 57, 91, 180];
        let new = [21, 45, 233, 0];
        store.set(old, true).unwrap();
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.data_mut(|d| {
            d.insert_temp(
                key(),
                Shelf {
                    store: Some(store.clone()),
                    colors: vec![old],
                    ..Default::default()
                },
            )
        });
        let mut color = new;
        let frame = |color: &mut [u8; 4], events| {
            let mut id = egui::Id::NULL;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(850.0, 700.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    id = rgba_live(ui, color).id;
                },
            );
            (id, PaintedOutput(output))
        };
        let (id, _) = frame(&mut color, vec![]);
        let popup = id.with("rgba-popup");
        egui::Popup::open_id(&ctx, popup);
        let click = |color: &mut [u8; 4], p, button| {
            for pressed in [true, false] {
                frame(
                    color,
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
        };
        for (saved, action) in [
            (old, "Update from Current Color"),
            (new, "Remove from My Colors"),
        ] {
            frame(&mut color, vec![]);
            frame(&mut color, vec![]);
            let swatch = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("preset-swatch", hex(saved))))
                    .unwrap()
            });
            click(&mut color, swatch.center(), PointerButton::Secondary);
            frame(&mut color, vec![]);
            let (_, output) = frame(&mut color, vec![]);
            assert!(egui::Popup::is_id_open(&ctx, popup));
            let target = super::super::tests::text_rect(&output.shapes, action).unwrap();
            assert!(!dismissing_live_popup(&ctx));
            click(&mut color, target.center(), PointerButton::Primary);
            frame(&mut color, vec![]);
            assert!(
                egui::Popup::is_id_open(&ctx, popup),
                "{action} closed parent"
            );
            assert!(!context_open(&ctx));
            assert_eq!(color, new);
            assert_eq!(
                store.read().unwrap(),
                if saved == old { vec![new] } else { vec![] }
            );
        }
        click(&mut color, egui::pos2(800.0, 650.0), PointerButton::Primary);
        frame(&mut color, vec![]);
        assert!(!egui::Popup::is_id_open(&ctx, popup));
        assert_eq!(color, new);
    }
    #[test]
    fn saved_color_plus_click_reuses_exact_rgba_and_keeps_document_history_separate() {
        use egui::{Event, Modifiers, PointerButton};
        let temp = tempfile::tempdir().unwrap();
        let store = capture_core::saved_colors::Store::open(temp.path()).unwrap();
        let ctx = egui::Context::default();
        ctx.data_mut(|d| {
            d.insert_temp(
                key(),
                Shelf {
                    store: Some(store.clone()),
                    ..Default::default()
                },
            )
        });
        let mut draft = ColorDraft::new([17, 85, 201, 0]);
        let frame = |draft: &mut ColorDraft, events| {
            crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300.0, 200.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| show(ui, draft, false),
            ))
        };
        frame(&mut draft, vec![]);
        let out = frame(&mut draft, vec![]);
        let plus = super::super::tests::text_rect(&out.shapes, "+")
            .unwrap()
            .center();
        for pressed in [true, false] {
            frame(
                &mut draft,
                vec![
                    Event::PointerMoved(plus),
                    Event::PointerButton {
                        pos: plus,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(store.read().unwrap(), [[17, 85, 201, 0]]);
        assert_eq!(draft.color, [17, 85, 201, 0]);
        let saved = capture_core::saved_colors::Store::open(temp.path())
            .unwrap()
            .read()
            .unwrap();
        draft.set_color([255; 4]);
        draft.set_color(saved[0]);
        assert_eq!(draft.color, [17, 85, 201, 0]);
    }
}
