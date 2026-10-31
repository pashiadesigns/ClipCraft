use crate::{Command, Editor, egui};
#[derive(Default)]
pub(super) struct Panel {
    pub active: bool,
    store: Option<capture_core::stamps::Library>,
    query: String,
    textures: std::collections::HashMap<String, egui::TextureHandle>,
    error: String,
}
impl Editor {
    pub(super) fn stamp_properties(&mut self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Stamp Library",
            "Import PNG or JPEG images. Click a stamp to place an editable copy at the center of the visible canvas. Drag or resize it with Move. Original files and saved stamps stay unchanged.",
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
        let entries = self
            .stamps
            .store
            .as_ref()
            .map(|s| {
                s.0.entries()
                    .iter()
                    .filter(|e| e.matches_query(&self.stamps.query))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
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
                            && let Some(store) = &self.stamps.store
                            && let Ok(pixels) = store.0.thumbnail(&entry.id)
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
            let result = self.stamps.store.as_ref().unwrap().payload(&id);
            match result {
                Ok(payload) => self.place_stamp(payload),
                Err(e) => self.stamps.error = e.to_string(),
            }
        }
    }
    fn place_stamp(&mut self, mut payload: capture_core::clipboard_payload::ObjectPayload) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        self.cancel_gesture();
        let center = self
            .view
            .current_transform()
            .map(|t| t.image_point(t.viewport.center()))
            .unwrap_or([
                self.session.document.width as f32 * 0.5,
                self.session.document.height as f32 * 0.5,
            ]);
        // Preserve aspect and original pixels while giving large imports a manageable frame.
        if let Some(a) = payload.objects.first_mut() {
            let w = (a.end[0] - a.start[0]).abs();
            let h = (a.end[1] - a.start[1]).abs();
            let scale = (self.session.document.width as f32 * 0.7 / w)
                .min(self.session.document.height as f32 * 0.7 / h)
                .min(1.);
            a.end = [a.start[0] + w * scale, a.start[1] + h * scale];
        }
        let before = self.session.document.annotations.len();
        self.execute(Command::PasteObjects { payload, center });
        if self.session.document.annotations.len() > before {
            self.select_only(self.session.document.annotations.last().map(|a| a.id));
            self.stamps.active = false;
            self.tool = None;
            self.status = "Stamp placed. Drag or resize it; Undo removes this copy.".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        e.place_stamp(payload);
        assert_eq!(e.session.document.annotations.len(), before + 1);
        assert!(e.selected.is_some());
        assert!(!e.stamps.active);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), before);
    }
}
