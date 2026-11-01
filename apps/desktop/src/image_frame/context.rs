use super::*;
use crate::clipboard::objects::Transfer;
use capture_core::{
    Document,
    bitmap::Bitmap,
    templates::{self, Scale},
};
use std::sync::Arc;
#[derive(Clone, Default)]
struct LibraryPicker {
    id: u64,
    query: String,
    error: String,
    focus: bool,
}
impl Editor {
    pub(crate) fn frame_context(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        transform: crate::canvas_view::Transform,
    ) {
        let key = egui::Id::unique("image-frame-context-target");
        if response.secondary_clicked() {
            let hit = response.interact_pointer_pos().and_then(|p| {
                self.session
                    .document
                    .annotations
                    .iter()
                    .rev()
                    .find(|a| {
                        (a.is_shape() || a.is_image_frame())
                            && self.session.document.object_editable(a.id)
                            && a.hit_test(transform.image_point(p), 2. / transform.scale)
                    })
                    .map(|a| a.id)
            });
            ui.ctx().data_mut(|d| {
                d.remove::<u64>(key);
                if let Some(id) = hit {
                    d.insert_temp(key, id);
                }
            });
            if let Some(id) = hit {
                self.select_only(Some(id));
            }
        }
        let Some(id) = ui.ctx().data(|d| d.get_temp::<u64>(key)) else {
            return;
        };
        let Some(a) = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id)
            .cloned()
        else {
            return;
        };
        let mut action = None;
        response.context_menu(|ui| {
            if !a.is_image_frame() {
                if ui.button("Convert to image frame").clicked() {
                    action = Some("Convert to image frame");
                    ui.close();
                }
                return;
            }
            ui.menu_button("Replace Image", |ui| {
                for label in ["From File…", "From Clipboard", "From Library…"] {
                    if ui.button(label).clicked() {
                        action = Some(label);
                        ui.close();
                    }
                }
            });
            let empty = a.image.as_ref().is_some_and(|b| b.is_placeholder());
            for (label, enabled) in [
                ("Clear Image", !empty),
                ("Crop Image", !empty),
                ("Reset Crop", !empty),
                ("Copy Image", !empty),
                ("Convert to basic shape", true),
                ("Delete Frame", true),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    action = Some(label);
                    ui.close();
                }
            }
        });
        self.frame_action(ui, id, action);
    }
    pub(super) fn choose_frame_source(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        id: u64,
    ) {
        let mut action = None;
        egui::Popup::menu(response).show(|ui| {
            for label in ["From File…", "From Clipboard", "From Library…"] {
                if ui.button(label).clicked() {
                    action = Some(label);
                    ui.close();
                }
            }
        });
        self.frame_action(ui, id, action);
    }
    fn frame_action(&mut self, ui: &mut egui::Ui, id: u64, action: Option<&str>) {
        let Some(a) = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id)
            .cloned()
        else {
            return;
        };
        match action {
            Some("Convert to image frame") | Some("Convert to basic shape") => {
                self.convert_selected_frame(action == Some("Convert to image frame"));
            }
            Some("From File…") => {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::unique("replace-frame-image"), id));
            }
            Some("From Clipboard") => {
                let result = self.replace_frame_from_clipboard(
                    id,
                    &mut crate::clipboard::objects::SystemTransfer {
                        owner: self.hotkey_owner,
                    },
                );
                if let Err(e) = result {
                    self.status = e;
                }
            }
            Some("From Library…") => {
                self.history.refresh();
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::unique("frame-library-picker"),
                        LibraryPicker {
                            id,
                            query: String::new(),
                            error: String::new(),
                            focus: true,
                        },
                    )
                });
            }
            Some("Clear Image") => {
                if let Err(e) = templates::placeholder()
                    .map_err(|e| e.to_string())
                    .and_then(|b| self.replace_frame_bitmap(id, b))
                {
                    self.status = e;
                }
            }
            Some("Crop Image") => {
                ui.ctx().data_mut(|d| d.insert_temp(crop_mode_key(), id));
            }
            Some("Reset Crop") => {
                if let Some(b) = a.image.as_ref() {
                    match b.with_view(None) {
                        Ok(b) => {
                            let mut a = a;
                            a.image = Some(Arc::new(b));
                            self.execute(Command::Update { annotation: a });
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
            }
            Some("Copy Image") => self.copy_canvas_selection(false),
            Some("Delete Frame") => self.execute(Command::Remove { id }),
            _ => {}
        }
    }
    pub(crate) fn replace_frame_bitmap(
        &mut self,
        id: u64,
        bitmap: Arc<Bitmap>,
    ) -> Result<(), String> {
        self.commit_property_preview();
        let old = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id && a.is_image_frame() && self.session.document.object_editable(id))
            .ok_or("Image frame is no longer editable.")?;
        let mut a = old.clone();
        let bitmap = if a.is_shape() {
            Arc::new(bitmap.with_frame_opacity(a.image_opacity()))
        } else {
            bitmap
        };
        let (lo, hi) = a.body_bounds();
        a.image = templates::image_in_frame(
            bitmap,
            id,
            [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]],
            Scale::Fill,
        )
        .map_err(|e| e.to_string())?
        .image;
        self.execute(Command::Update { annotation: a });
        Ok(())
    }
    fn replace_frame_from_clipboard(
        &mut self,
        id: u64,
        transfer: &mut impl Transfer,
    ) -> Result<(), String> {
        let payload = transfer.read()?;
        payload.validate().map_err(|e| e.to_string())?;
        let bitmap = match payload.content {
            capture_core::clipboard_payload::Content::Image(i) => Bitmap::new(i.image)
                .map(Arc::new)
                .map_err(|e| e.to_string())?,
            capture_core::clipboard_payload::Content::Objects(o) => {
                let mut d = Document::canvas(o.source_canvas[0], o.source_canvas[1], [0; 4])
                    .map_err(|e| e.to_string())?;
                d.version = o.document_version;
                d.annotations = o.objects;
                d.promote_features();
                let ids = d.annotations.iter().map(|a| a.id).collect::<Vec<_>>();
                let (_, pixels) = crate::clipboard::objects::selection(&d, &ids)?;
                Arc::new(
                    Bitmap::new(
                        capture_core::clipboard_payload::ImagePayload::from_rgba(
                            pixels.width(),
                            pixels.height(),
                            pixels.into_raw(),
                        )
                        .map(|i| i.image)
                        .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?,
                )
            }
        };
        self.replace_frame_bitmap(id, bitmap)
    }
    pub(crate) fn show_frame_library_picker(&mut self, ctx: &egui::Context) {
        let key = egui::Id::unique("frame-library-picker");
        let Some(mut p) = ctx.data_mut(|d| d.remove_temp::<LibraryPicker>(key)) else {
            return;
        };
        let store = self
            .history
            .root()
            .and_then(|root| capture_core::history::History::open(&root).ok());
        let mut selected = None;
        let mut cancel = false;
        let modal = egui::Modal::new(key).show(ctx, |ui| {
            ui.set_width(560.);
            ui.heading("Replace Image from Library");
            ui.label("Search");
            let r = ui.add(egui::TextEdit::singleline(&mut p.query).desired_width(f32::INFINITY));
            crate::controls::focus_once(&r, &mut p.focus);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height((ctx.content_rect().height() - 190.).max(120.))
                .show(ui, |ui| {
                    if let Some(s) = &store {
                        let entries = s
                            .entries()
                            .iter()
                            .filter(|e| e.matches_query(&p.query))
                            .collect::<Vec<_>>();
                        if entries.is_empty() {
                            ui.label("No matching images.");
                        }
                        let columns = ((ui.available_width() + 8.) / 88.).floor().max(1.) as usize;
                        for row in entries.chunks(columns) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.;
                                for entry in row {
                                    ui.vertical(|ui| {
                                        ui.set_width(80.);
                                        let response = self.history_card_action(
                                            ui,
                                            entry,
                                            egui::vec2(80., 48.),
                                            Some("Replace image"),
                                        );
                                        #[cfg(test)]
                                        ctx.data_mut(|d| {
                                            d.insert_temp(
                                                egui::Id::unique(("frame-library-card", &entry.id)),
                                                response.rect,
                                            )
                                        });
                                        if response.clicked() && self.history.available(&entry.id) {
                                            selected = Some(entry.id.clone());
                                        }
                                        ui.add(egui::Label::new(&entry.label).truncate())
                                            .on_hover_text(&entry.label);
                                    });
                                }
                            });
                            ui.add_space(12.);
                        }
                    } else {
                        ui.label("Library is unavailable.");
                    }
                });
            if !p.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &p.error);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if let Some(id) = selected {
            let result = store
                .unwrap()
                .load(&id)
                .and_then(|d| d.render())
                .and_then(|pixels| {
                    capture_core::clipboard_payload::ImagePayload::from_rgba(
                        pixels.width(),
                        pixels.height(),
                        pixels.into_raw(),
                    )
                    .map(|i| i.image)
                })
                .and_then(Bitmap::new)
                .map_err(|e| e.to_string())
                .and_then(|b| self.replace_frame_bitmap(p.id, Arc::new(b)));
            match result {
                Ok(()) => return,
                Err(e) => p.error = e,
            }
        }
        ctx.data_mut(|d| d.insert_temp(key, p));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    struct TransferFixture {
        image: Option<capture_core::clipboard_payload::ClipboardPayload>,
    }
    impl Transfer for TransferFixture {
        fn read(&mut self) -> Result<capture_core::clipboard_payload::ClipboardPayload, String> {
            self.image.take().ok_or("Synthetic empty clipboard".into())
        }
        fn write(
            &mut self,
            _: &capture_core::clipboard_payload::ClipboardPayload,
            _: &image::RgbaImage,
        ) -> Result<(), String> {
            panic!("Replacement must not write clipboard")
        }
    }
    #[test]
    fn frame_clipboard_replace_clear_undo_and_reopen_preserve_geometry() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut a = templates::image_in_frame(
            templates::placeholder().unwrap(),
            1,
            [20., 15., 220., 140.],
            Scale::Fill,
        )
        .unwrap();
        a.rotation_degrees = 9.;
        a.corner_radius = 17.;
        a.color[3] = 190;
        let mut doc = Document::canvas(400, 300, [255; 4]).unwrap();
        doc.annotations.push(a.clone());
        doc.promote_features();
        e.session = capture_core::Session::new(doc).unwrap();
        let pixels = image::RgbaImage::from_pixel(30, 80, image::Rgba([40, 80, 210, 180]));
        let payload =
            capture_core::clipboard_payload::ImagePayload::from_rgba(30, 80, pixels.into_raw())
                .unwrap();
        let mut t = TransferFixture {
            image: Some(capture_core::clipboard_payload::ClipboardPayload {
                schema_version: 1,
                content: capture_core::clipboard_payload::Content::Image(payload),
            }),
        };
        e.replace_frame_from_clipboard(1, &mut t).unwrap();
        let filled = e.session.document.annotations[0].clone();
        assert!(!filled.image.as_ref().unwrap().is_placeholder());
        assert_eq!(
            (
                filled.start,
                filled.end,
                filled.corner_radius,
                filled.rotation_degrees,
                filled.color
            ),
            (a.start, a.end, a.corner_radius, a.rotation_degrees, a.color)
        );
        assert!(e.replace_frame_from_clipboard(1, &mut t).is_err());
        assert_eq!(e.session.document.annotations[0], filled);
        e.replace_frame_bitmap(1, templates::placeholder().unwrap())
            .unwrap();
        assert!(
            e.session.document.annotations[0]
                .image
                .as_ref()
                .unwrap()
                .is_placeholder()
        );
        e.session.execute(Command::Undo).unwrap();
        assert_eq!(e.session.document.annotations[0], filled);
        e.session.execute(Command::Undo).unwrap();
        assert_eq!(e.session.document.annotations[0], a);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.capture.json");
        e.session.document.save(&path).unwrap();
        assert!(
            Document::load(&path).unwrap().annotations[0]
                .image
                .as_ref()
                .unwrap()
                .is_placeholder()
        );
    }
    #[test]
    fn actual_frame_menu_clears_image_in_one_step_and_library_picker_keeps_source() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        let pixels = image::RgbaImage::from_pixel(40, 30, image::Rgba([30, 120, 210, 255]));
        let payload =
            capture_core::clipboard_payload::ImagePayload::from_rgba(40, 30, pixels.into_raw())
                .unwrap();
        let a = templates::image_in_frame(
            Arc::new(Bitmap::new(payload.image).unwrap()),
            1,
            [50., 50., 250., 180.],
            Scale::Fill,
        )
        .unwrap();
        let mut d = Document::canvas(400, 300, [255; 4]).unwrap();
        d.annotations.push(a.clone());
        d.promote_features();
        e.session = capture_core::Session::new(d).unwrap();
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 580.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    let rect = ui.available_rect_before_wrap();
                    let response =
                        ui.interact(rect, egui::Id::unique("frame-test"), egui::Sense::click());
                    e.frame_context(
                        ui,
                        &response,
                        crate::canvas_view::Transform {
                            image: rect,
                            viewport: rect,
                            scale: 1.,
                            zoom: 1.,
                        },
                    );
                },
            ))
        };
        let mut first = frame(&mut e, vec![]);
        first
            .0
            .textures_delta
            .append(frame(&mut e, vec![]).0.textures_delta.clone());
        let pos = egui::pos2(70., 70.);
        for pressed in [true, false] {
            let out = frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            first.0.textures_delta.append(out.0.textures_delta.clone());
        }
        let out = frame(&mut e, vec![]);
        let clear = crate::color_widgets::tests::text_rect(&out.shapes, "Clear Image").unwrap();
        assert!(crate::color_widgets::tests::text_rect(&out.shapes, "Replace Image").is_some());
        first.0.textures_delta.append(out.0.textures_delta.clone());
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_RECOVERY_DIR") {
            dump_meshes(&ctx, &dir, "frame-menu", first, out);
        }
        let pos = clear.center();
        for pressed in [true, false] {
            frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(
            e.session.document.annotations[0]
                .image
                .as_ref()
                .unwrap()
                .is_placeholder()
        );
        e.session.execute(Command::Undo).unwrap();
        assert_eq!(e.session.document.annotations[0], a);
        let mut store = capture_core::history::History::open(root.path()).unwrap();
        let src = Document::canvas(80, 60, [200, 30, 80, 255]).unwrap();
        let entry = store
            .create(
                &src,
                "Library replacement",
                capture_core::history::Origin::Canvas,
            )
            .unwrap();
        let before = store.load(&entry.id).unwrap().render().unwrap();
        for index in 1..8 {
            store
                .create(
                    &Document::canvas(80, 60, [30 * index, 100, 170, 255]).unwrap(),
                    &format!("Sample {index}"),
                    capture_core::history::Origin::Canvas,
                )
                .unwrap();
        }
        e.history.refresh();
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("frame-library-picker"),
                LibraryPicker {
                    id: 1,
                    query: String::new(),
                    error: String::new(),
                    focus: true,
                },
            )
        });
        let picker = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 580.),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| e.show_frame_library_picker(ui.ctx()),
            ))
        };
        let mut initial = picker(&mut e, vec![]);
        let out = picker(&mut e, vec![]);
        let target = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("frame-library-card", &entry.id))))
            .unwrap();
        assert_eq!(target.size(), egui::vec2(80., 48.));
        assert!(crate::color_widgets::tests::text_rect(&out.shapes, "IMAGE").is_some());
        assert!(
            out.textures_delta
                .set
                .iter()
                .any(|(id, _)| *id != egui::TextureId::default())
                || initial
                    .textures_delta
                    .set
                    .iter()
                    .any(|(id, _)| *id != egui::TextureId::default())
        );
        initial
            .0
            .textures_delta
            .append(out.0.textures_delta.clone());
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_RECOVERY_DIR") {
            dump_meshes(&ctx, &dir, "frame-library-thumbnails", initial, out);
        }
        let target = target.center();
        for pressed in [true, false] {
            picker(
                &mut e,
                vec![
                    egui::Event::PointerMoved(target),
                    egui::Event::PointerButton {
                        pos: target,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(store.load(&entry.id).unwrap().render().unwrap(), before);
        assert_ne!(e.session.document.annotations[0].image, a.image);
        assert_eq!(e.session.document.annotations[0].start, a.start);
    }
}
