use crate::{Command, Editor, egui};
#[derive(Default)]
pub(super) struct Panel {
    pub active: bool,
    armed: Option<Armed>,
    store: Option<capture_core::stamps::Library>,
    query: String,
    textures: std::collections::HashMap<String, egui::TextureHandle>,
    error: String,
    pub anchor: Option<(crate::Kind, [f32; 2])>,
    placed: Vec<u64>,
    pub redact: Option<crate::Annotation>,
    pub redact_drag: Option<([f32; 2], crate::Annotation)>,
}
struct Choice {
    id: String,
    label: String,
}
struct Armed {
    id: String,
    payload: capture_core::clipboard_payload::ObjectPayload,
    preview: Option<(crate::Annotation, [u32; 2], egui::TextureHandle)>,
    placing: bool,
}
impl Panel {
    pub(super) fn reset_document(&mut self) {
        self.cancel_drag();
        self.placed.clear();
        self.anchor = None;
    }

    pub(super) fn cancel_drag(&mut self) {
        self.redact_drag = None;
        if let Some(armed) = &mut self.armed {
            armed.placing = false;
        }
    }
    pub(super) fn disarm(&mut self) {
        self.cancel_drag();
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
        _ctx: &egui::Context,
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
        self.select_only(None);
        self.stamps.active = true;
        self.tool = None;
        self.stamps.placed.clear();
        self.stamps.anchor = None;
        self.stamps.armed = Some(Armed {
            id,
            payload,
            preview: None,
            placing: false,
        });
    }

    /// Picking/editing an existing stamp establishes the next placement template.
    pub(super) fn remember_placement_template(&mut self, annotation: &crate::Annotation) {
        if annotation.kind == crate::Kind::Redact {
            self.stamps.redact = Some(annotation.clone());
        }
        if annotation.kind == crate::Kind::Step {
            self.step_size = (annotation.end[0] - annotation.start[0]).abs().round() as u32;
            self.step_letters = annotation
                .text
                .as_ref()
                .is_some_and(|t| t.content.chars().any(char::is_alphabetic));
            self.remember_annotation_style(annotation);
            self.stamps.anchor = Some((crate::Kind::Step, annotation_center(annotation)));
        } else if annotation.image.as_ref().is_some_and(|b| b.is_stamp()) {
            let mut a = annotation.clone();
            a.group = None;
            let unchanged = self
                .stamps
                .armed
                .as_ref()
                .is_some_and(|v| v.payload.objects[0] == a);
            if !unchanged {
                let name = a
                    .image
                    .as_ref()
                    .and_then(|b| b.stamp_name())
                    .unwrap_or("Imported stamp");
                let id = capture_core::stamps::builtin::ALL
                    .iter()
                    .find(|s| s.label == name)
                    .map_or_else(|| name.to_owned(), |s| s.id.to_owned());
                self.stamps.armed = Some(Armed {
                    id,
                    payload: capture_core::clipboard_payload::ObjectPayload {
                        document_version: self.session.document.version,
                        source_canvas: [self.session.document.width, self.session.document.height],
                        objects: vec![a],
                    },
                    preview: None,
                    placing: false,
                });
            }
            self.stamps.anchor = Some((crate::Kind::Image, annotation_center(annotation)));
        }
    }
    pub(super) fn constrain_placement(
        &self,
        kind: crate::Kind,
        mut point: [f32; 2],
        shift: bool,
    ) -> [f32; 2] {
        if shift
            && let Some((anchor_kind, center)) = self.stamps.anchor
            && anchor_kind == kind
        {
            let axis = usize::from((point[0] - center[0]).abs() > (point[1] - center[1]).abs());
            point[axis] = center[axis];
        }
        point
    }
    pub(super) fn stamp_placement_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: crate::canvas_view::Transform,
        interactive: bool,
    ) -> bool {
        if !self.stamps.active {
            return false;
        }
        if let Some(a) = self
            .selected_annotation()
            .filter(|a| a.image.as_ref().is_some_and(|b| b.is_stamp()))
        {
            self.remember_placement_template(&a);
        }
        if self.stamps.armed.is_none() {
            let stamp = capture_core::stamps::builtin::find("builtin-check-green").unwrap();
            if let Ok(payload) = stamp.payload() {
                self.arm_stamp(ui.ctx(), stamp.id.into(), payload);
            }
            if self.stamps.armed.is_none() {
                return false;
            }
        }
        let (focused, pressed, released, point, space, shift) = ui.input(|i| {
            (
                i.focused,
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.interact_pos(),
                i.key_down(egui::Key::Space),
                i.modifiers.shift,
            )
        });
        if !focused {
            self.stamps.cancel_drag();
            return true;
        }
        if !interactive || space || self.view.panning {
            return false;
        }
        let visible = transform.image.intersect(transform.viewport);
        // Selected stamps use the normal move/resize handles. An empty press resumes stamping.
        if pressed
            && let Some(origin) =
                crate::step_tool::placement_origin(ui).filter(|p| visible.contains(*p))
        {
            let hit = self
                .session
                .document
                .annotations
                .iter()
                .rev()
                .find(|a| {
                    a.image.as_ref().is_some_and(|b| b.is_stamp())
                        && self.session.document.object_editable(a.id)
                        && a.hit_test(transform.image_point(origin), 7. / transform.scale)
                })
                .map(|a| a.id);
            if let Some(id) = hit {
                if self.selected == Some(id) {
                    return false;
                }
                self.select_only(Some(id));
                return true;
            }
            if self.edit_drag.is_some() || self.rotation_drag.is_some() {
                return false;
            }
            self.commit_property_preview();
            self.select_only(None);
            self.stamps.armed.as_mut().unwrap().placing = true;
        }
        if self.selected.is_some() || self.edit_drag.is_some() {
            return false;
        }
        let Some(point) = point else {
            return true;
        };
        let placing = self.stamps.armed.as_ref().unwrap().placing;
        let original = self.stamps.armed.as_ref().unwrap().payload.objects[0].clone();
        let mut center =
            self.constrain_placement(crate::Kind::Image, transform.image_point(point), shift);
        let previous = annotation_center(&original);
        let mut probe = original.clone();
        probe.id = self.session.next_id();
        if let Ok(result) = crate::canvas_expansion::move_with_candidates(
            &self.session.document,
            std::slice::from_ref(&probe),
            [center[0] - previous[0], center[1] - previous[1]],
            self.quick_controls
                .snapping()
                .then_some(6. / transform.scale),
            true,
            self.quick_controls.snap_candidates(),
        ) {
            center = self.constrain_placement(
                crate::Kind::Image,
                annotation_center(&result.annotations[0]),
                shift,
            );
            self.snapping_guides = result.guides;
        }
        let painter = ui.painter().with_clip_rect(transform.viewport);
        // Temporary boundaries make repeat-placement alignment targets visible.
        self.stamps.placed.retain(|id| {
            self.session
                .document
                .annotations
                .iter()
                .any(|a| a.id == *id)
        });
        for a in self.session.document.annotations.iter().filter(|a| {
            self.stamps.placed.contains(&a.id) && self.session.document.object_visible(a.id)
        }) {
            let (lo, hi) = a.bounds();
            let rect =
                egui::Rect::from_min_max(transform.screen_point(lo), transform.screen_point(hi));
            painter.rect_stroke(
                rect,
                0.,
                egui::Stroke::new(1., crate::shell::ACCENT.gamma_multiply(0.65)),
                egui::StrokeKind::Inside,
            );
        }
        if (response.hovered() || placing) && visible.contains(point) {
            let armed = self.stamps.armed.as_mut().unwrap();
            paint_preview(ui, transform, armed, center);
            for axis in 0..2 {
                if let Some(value) = self.snapping_guides[axis] {
                    let points = if axis == 0 {
                        [
                            transform.screen_point([value, 0.]),
                            transform.screen_point([value, self.session.document.height as f32]),
                        ]
                    } else {
                        [
                            transform.screen_point([0., value]),
                            transform.screen_point([self.session.document.width as f32, value]),
                        ]
                    };
                    painter.line_segment(points, egui::Stroke::new(1., crate::shell::ACCENT));
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        }
        if released && placing {
            self.stamps.cancel_drag();
            if visible.contains(point) {
                self.place_stamp(center);
            }
        }
        true
    }

    fn place_stamp(&mut self, center: [f32; 2]) {
        let Some(armed) = &self.stamps.armed else {
            return;
        };
        let payload = armed.payload.clone();
        let before = self.session.document.annotations.len();
        self.execute(Command::PasteObjects { payload, center });
        if self.session.document.annotations.len() > before {
            let a = self.session.document.annotations.last().unwrap();
            self.stamps.placed.push(a.id);
            self.quick_controls.touch_snap(a.id);
            self.stamps.anchor = Some((crate::Kind::Image, annotation_center(a)));
        }
        self.select_only(None);
        self.status = "Stamp placed. Click to place another; select a stamp to edit it.".into();
    }
}

fn annotation_center(a: &crate::Annotation) -> [f32; 2] {
    let (lo, hi) = a.bounds();
    [0, 1].map(|i| (lo[i] + hi[i]) * 0.5)
}
fn paint_preview(
    ui: &egui::Ui,
    transform: crate::canvas_view::Transform,
    armed: &mut Armed,
    center: [f32; 2],
) {
    let a = &armed.payload.objects[0];
    let (lo, hi) = a.bounds();
    let extent = egui::vec2(hi[0] - lo[0], hi[1] - lo[1]);
    let scale = transform.scale * ui.ctx().pixels_per_point();
    let size = extent * scale;
    let quality = (2048. / size.max_elem())
        .min((2_000_000. / (size.x * size.y)).sqrt())
        .min(1.);
    let density = scale * quality;
    let pixels = [
        (size.x * quality).ceil().max(1.) as u32,
        (size.y * quality).ceil().max(1.) as u32,
    ];
    if armed
        .preview
        .as_ref()
        .is_none_or(|(previous, key, _)| previous != a || *key != pixels)
    {
        let mut local = a.clone();
        local.start = [a.start[0] - lo[0], a.start[1] - lo[1]];
        local.end = [a.end[0] - lo[0], a.end[1] - lo[1]];
        if let Ok(pixels_image) = capture_core::display::render_annotation_preview(
            &local,
            capture_core::display::Viewport {
                size: pixels,
                origin: [0.; 2],
                scale: f64::from(density),
            },
        ) {
            let texture = ui.ctx().load_texture(
                "stamp-vector-preview",
                egui::ColorImage::from_rgba_unmultiplied(
                    [pixels[0] as usize, pixels[1] as usize],
                    pixels_image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            armed.preview = Some((a.clone(), pixels, texture));
        }
    }
    if let Some((_, _, texture)) = &armed.preview {
        ui.painter().with_clip_rect(transform.viewport).image(
            texture.id(),
            egui::Rect::from_center_size(transform.screen_point(center), extent * transform.scale),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_stamp_edits_seed_repeat_placement_and_vector_preview_cache_tracks_zoom_only() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let stamp = capture_core::stamps::builtin::find("builtin-five-color").unwrap();
        e.arm_stamp(&ctx, stamp.id.into(), stamp.payload().unwrap());
        e.place_stamp([110., 110.]);
        let first = e.session.document.annotations[0].clone();
        e.select_only(Some(first.id));
        let mut edited = first.clone();
        edited.end = [edited.start[0] + 90., edited.start[1] + 90.];
        let bitmap = edited.image.as_ref().unwrap();
        let mut vector = bitmap.vector().unwrap().clone();
        assert_eq!(vector.colors.len(), 5);
        vector.colors[4] = [125, 20, 230, 180];
        edited.image = Some(std::sync::Arc::new(
            capture_core::bitmap::Bitmap::from_vector(vector)
                .unwrap()
                .named_stamp("Five-color workflow"),
        ));
        e.execute(Command::Update {
            annotation: edited.clone(),
        });
        assert_eq!(e.stamps.armed.as_ref().unwrap().payload.objects[0], edited);
        e.select_only(None);
        e.place_stamp([300., 120.]);
        let placed = e.session.document.annotations.last().unwrap();
        assert_eq!(placed.image, edited.image);
        assert_eq!(placed.end[0] - placed.start[0], 90.);
        assert_eq!(
            e.constrain_placement(crate::Kind::Image, [302., 300.], true),
            [300., 300.]
        );
        assert_eq!(
            e.constrain_placement(crate::Kind::Image, [500., 121.], true),
            [500., 120.]
        );
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), 1);
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 700.));
        let draw = |e: &mut Editor, scale: f32, center: [f32; 2]| {
            crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(area),
                    ..Default::default()
                },
                |ui| {
                    paint_preview(
                        ui,
                        crate::canvas_view::Transform {
                            image: area,
                            viewport: area,
                            scale,
                            zoom: scale,
                        },
                        e.stamps.armed.as_mut().unwrap(),
                        center,
                    );
                },
            ))
        };
        draw(&mut e, 4., [50., 50.]);
        let size = e.stamps.armed.as_ref().unwrap().preview.as_ref().unwrap().1;
        assert_eq!(size, [360, 360]);
        let moved = draw(&mut e, 4., [80., 65.]);
        assert!(
            moved.textures_delta.set.is_empty(),
            "pointer movement must reuse rendered paths"
        );
        draw(&mut e, 2., [80., 65.]);
        assert_eq!(
            e.stamps.armed.as_ref().unwrap().preview.as_ref().unwrap().1,
            [180, 180]
        );
    }
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
        assert!(e.stamps.armed.is_some());
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
