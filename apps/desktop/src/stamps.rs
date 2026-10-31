use crate::{Command, Editor, egui};
#[derive(Default)]
pub(super) struct Panel {
    pub active: bool,
    armed: Option<Armed>,
    store: Option<capture_core::stamps::Library>,
    query: String,
    textures: std::collections::HashMap<String, egui::TextureHandle>,
    error: String,
}
struct Choice {
    id: String,
    label: String,
}
struct Armed {
    id: String,
    payload: capture_core::clipboard_payload::ObjectPayload,
    texture: egui::TextureHandle,
    placing: bool,
}
impl Panel {
    pub(super) fn cancel_drag(&mut self) {
        if let Some(armed) = &mut self.armed {
            armed.placing = false;
        }
    }
    pub(super) fn disarm(&mut self) {
        self.armed = None;
    }
}
impl Editor {
    pub(super) fn stamp_properties(&mut self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Stamp Library",
            "Import PNG or JPEG images. Choose a stamp, then click or drag on the canvas to place copies. Each release adds one copy. Select a placed stamp or choose Move to edit it. Original files and saved stamps stay unchanged.",
        );
        if self.stamps.store.is_none() {
            if let Some(root) = self.history.root() {
                match capture_core::stamps::Library::open(&root.join("stamps")) {
                    Ok(store) => self.stamps.store = Some(store),
                    Err(e) => self.stamps.error = e.to_string(),
                }
            } else {
                self.stamps.error = "Local storage is unavailable.".into();
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Import images…").clicked() {
                self.suspend_global_capture();
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
                    .pick_files()
                    && let Some(store) = &mut self.stamps.store
                {
                    let mut count = 0;
                    for path in paths {
                        match store.import(&path) {
                            Ok(_) => count += 1,
                            Err(e) => {
                                self.stamps.error = e.to_string();
                                break;
                            }
                        }
                    }
                    self.status = format!("Imported {count} stamp(s).");
                }
            }
            if ui.button("Refresh").clicked() {
                if let Some(store) = &mut self.stamps.store
                    && let Err(e) = store.0.refresh()
                {
                    self.stamps.error = e.to_string();
                }
                self.stamps.textures.clear();
            }
        });
        ui.label("Search stamps");
        ui.text_edit_singleline(&mut self.stamps.query);
        if !self.stamps.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.stamps.error);
        }
        let mut entries: Vec<Choice> = capture_core::stamps::builtin::ALL
            .iter()
            .filter(|s| s.matches_query(&self.stamps.query))
            .map(|s| Choice {
                id: s.id.into(),
                label: s.label.into(),
            })
            .collect();
        entries.extend(
            self.stamps
                .store
                .as_ref()
                .map(|s| {
                    s.0.entries()
                        .iter()
                        .filter(|e| e.matches_query(&self.stamps.query))
                        .map(|e| Choice {
                            id: e.id.clone(),
                            label: e.label.clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        if entries.is_empty() {
            ui.label("No matching stamps.");
        }
        let mut place = None;
        egui::Grid::new("stamp-grid")
            .num_columns(2)
            .spacing(egui::vec2(8., 8.))
            .show(ui, |ui| {
                for (i, entry) in entries.iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(106.);
                        if !self.stamps.textures.contains_key(&entry.id)
                            && let Some(pixels) = capture_core::stamps::builtin::find(&entry.id)
                                .and_then(|s| s.pixels().ok())
                                .or_else(|| {
                                    self.stamps
                                        .store
                                        .as_ref()
                                        .and_then(|s| s.0.thumbnail(&entry.id).ok())
                                })
                        {
                            let texture = ui.ctx().load_texture(
                                format!("stamp-{}", entry.id),
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [pixels.width() as usize, pixels.height() as usize],
                                    pixels.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            );
                            self.stamps.textures.insert(entry.id.clone(), texture);
                        }
                        if let Some(texture) = self.stamps.textures.get(&entry.id) {
                            let response = ui
                                .add(
                                    egui::Button::image(
                                        egui::Image::new(texture)
                                            .fit_to_exact_size(egui::vec2(96., 60.)),
                                    )
                                    .selected(
                                        self.stamps
                                            .armed
                                            .as_ref()
                                            .is_some_and(|a| a.id == entry.id),
                                    )
                                    .min_size(egui::vec2(106., 70.)),
                                )
                                .on_hover_text(format!("Place {}", entry.label));
                            if response.clicked() {
                                place = Some(entry.id.clone());
                            }
                        }
                        ui.add(egui::Label::new(&entry.label).truncate())
                            .on_hover_text(&entry.label);
                    });
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        if let Some(id) = place {
            let result = if let Some(stamp) = capture_core::stamps::builtin::find(&id) {
                stamp.payload()
            } else if let Some(store) = &self.stamps.store {
                store.payload(&id)
            } else {
                return;
            };
            match result {
                Ok(payload) => self.arm_stamp(ui.ctx(), id, payload),
                Err(e) => self.stamps.error = e.to_string(),
            }
        }
    }
    fn arm_stamp(
        &mut self,
        ctx: &egui::Context,
        id: String,
        mut payload: capture_core::clipboard_payload::ObjectPayload,
    ) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        self.cancel_gesture();
        let Some(a) = payload.objects.first_mut() else {
            return;
        };
        let w = (a.end[0] - a.start[0]).abs();
        let h = (a.end[1] - a.start[1]).abs();
        let scale = (self.session.document.width as f32 * 0.7 / w)
            .min(self.session.document.height as f32 * 0.7 / h)
            .min(1.);
        a.end = [a.start[0] + w * scale, a.start[1] + h * scale];
        let Some(bitmap) = &a.image else {
            return;
        };
        let Ok(pixels) = bitmap.pixels() else {
            return;
        };
        let thumb = image::imageops::thumbnail(pixels.as_ref(), 512, 512);
        let texture = ctx.load_texture(
            "stamp-placement",
            egui::ColorImage::from_rgba_unmultiplied(
                [thumb.width() as usize, thumb.height() as usize],
                thumb.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        self.select_only(None);
        self.stamps.active = true;
        self.tool = None;
        self.stamps.armed = Some(Armed {
            id,
            payload,
            texture,
            placing: false,
        });
    }

    pub(super) fn stamp_placement_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: crate::canvas_view::Transform,
        interactive: bool,
    ) -> bool {
        if !self.stamps.active || self.stamps.armed.is_none() {
            return false;
        }
        let (focused, pressed, released, point, space, escape) = ui.input(|i| {
            (
                i.focused,
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.interact_pos(),
                i.key_down(egui::Key::Space),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if !focused || escape {
            self.stamps.cancel_drag();
            return true;
        }
        if !interactive || space || self.view.panning {
            return false;
        }
        let visible = transform.image.intersect(transform.viewport);
        if pressed
            && let Some(origin) =
                crate::step_tool::placement_origin(ui).filter(|p| visible.contains(*p))
        {
            // Only actual stamps take selection precedence; a base screenshot
            // must remain a valid placement surface.
            let hit = self
                .session
                .document
                .annotations
                .iter()
                .rev()
                .find(|a| {
                    a.image.as_ref().is_some_and(|image| image.is_stamp())
                        && self.session.document.object_editable(a.id)
                        && a.hit_test(transform.image_point(origin), 7. / transform.scale)
                })
                .map(|a| a.id);
            if let Some(id) = hit {
                self.stamps.disarm();
                self.select_only(Some(id));
                return true;
            }
            self.select_only(None);
            self.stamps.armed.as_mut().unwrap().placing = true;
        }
        let Some(point) = point else {
            return true;
        };
        let armed = self.stamps.armed.as_ref().unwrap();
        let placing = armed.placing;
        if (response.hovered() || placing) && visible.contains(point) {
            let a = &armed.payload.objects[0];
            let size = egui::vec2(a.end[0] - a.start[0], a.end[1] - a.start[1]) * transform.scale;
            let painter = ui.painter().with_clip_rect(transform.viewport);
            painter.image(
                armed.texture.id(),
                egui::Rect::from_center_size(point, size),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
            // Paint hotspot and image together, preventing OS/app cursor drift.
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            painter.circle_stroke(point, 3., egui::Stroke::new(1., egui::Color32::BLACK));
            painter.circle_stroke(point, 4., egui::Stroke::new(1., egui::Color32::WHITE));
        }
        if released && placing {
            self.stamps.cancel_drag();
            if visible.contains(point) {
                self.place_stamp(transform.image_point(point));
            }
        }
        true
    }

    fn place_stamp(&mut self, center: [f32; 2]) {
        let Some(armed) = &self.stamps.armed else {
            return;
        };
        let payload = armed.payload.clone();
        self.execute(Command::PasteObjects { payload, center });
        self.select_only(None);
        self.status = "Stamp placed. Click to place another; select a stamp to edit it.".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stamp_pointer_preview_release_repeat_and_reselection_match_step_flow() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let stamp = &capture_core::stamps::builtin::ALL[0];
        e.arm_stamp(&ctx, stamp.id.into(), stamp.payload().unwrap());
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640., 400.));
        let transform = crate::canvas_view::Transform {
            image: area,
            viewport: area,
            scale: 1.,
            zoom: 100.,
        };
        let frame = |e: &mut Editor, events| {
            crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(area),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    let response = ui.interact(
                        area,
                        egui::Id::unique("stamp-test-canvas"),
                        egui::Sense::click_and_drag(),
                    );
                    e.stamp_placement_input(ui, &response, transform, true);
                },
            ))
        };
        let button = |x, y, pressed| egui::Event::PointerButton {
            pos: egui::pos2(x, y),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = frame(&mut e, vec![]);
        let _ = frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(egui::pos2(90., 90.)),
                button(90., 90., true),
            ],
        );
        assert!(e.session.document.annotations.is_empty());
        let preview = frame(
            &mut e,
            vec![egui::Event::PointerMoved(egui::pos2(130., 110.))],
        );
        assert!(!preview.shapes.is_empty());
        assert!(e.session.document.annotations.is_empty());
        let _ = frame(&mut e, vec![button(130., 110., false)]);
        assert_eq!(e.session.document.annotations.len(), 1);
        assert!(e.stamps.active && e.selected.is_none());
        let a = &e.session.document.annotations[0];
        assert_eq!(
            [(a.start[0] + a.end[0]) * 0.5, (a.start[1] + a.end[1]) * 0.5],
            [130., 110.]
        );
        let first = a.id;
        let _ = frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(egui::pos2(340., 200.)),
                button(340., 200., true),
                button(340., 200., false),
            ],
        );
        assert_eq!(e.session.document.annotations.len(), 2);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), 1);
        // Click a previously placed stamp to leave placement and edit its frame.
        let _ = frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(egui::pos2(130., 110.)),
                button(130., 110., true),
            ],
        );
        assert_eq!(e.selected, Some(first));
        assert!(e.stamps.armed.is_none());
        assert!(e.stamps.active);
    }

    #[test]
    fn stamp_panel_search_and_placement_use_independent_image_objects() {
        use crate::property_layout::tests::{PaintedOutput, dump_meshes};
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.history = crate::history::HistoryPanel::isolated(root.path());
        let source = root.path().join("Blue badge.png");
        image::RgbaImage::from_fn(120, 80, |x, y| {
            if (20..100).contains(&x) && (10..70).contains(&y) {
                image::Rgba([35, 150, 215, 255])
            } else {
                image::Rgba([0; 4])
            }
        })
        .save(&source)
        .unwrap();
        let mut store = capture_core::stamps::Library::open(&root.path().join("stamps")).unwrap();
        let entry = store.import(&source).unwrap();
        e.stamps.store = Some(store);
        e.stamps.query = "Blue badge".into();
        e.stamps.active = true;
        let frame = |e: &mut Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(262., 450.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    e.properties(ui);
                },
            ))
        };
        let first = frame(&mut e);
        let output = frame(&mut e);
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Blue badge").is_some());
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_BETA_DIR") {
            dump_meshes(&ctx, &dir, "Stamps", first, output);
        }
        e.stamps.query = "no match".into();
        let output = frame(&mut e);
        assert!(
            crate::color_widgets::tests::text_rect(&output.shapes, "No matching stamps.").is_some()
        );
        let payload = e.stamps.store.as_ref().unwrap().payload(&entry.id).unwrap();
        let before = e.session.document.annotations.len();
        e.arm_stamp(&ctx, entry.id.clone(), payload);
        assert_eq!(e.session.document.annotations.len(), before);
        assert!(!e.session.can_undo());
        e.place_stamp([120., 120.]);
        assert_eq!(e.session.document.annotations.len(), before + 1);
        assert!(e.selected.is_none());
        assert!(e.stamps.active);
        e.place_stamp([300., 200.]);
        assert_eq!(e.session.document.annotations.len(), before + 2);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), before + 1);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), before);
    }
}
