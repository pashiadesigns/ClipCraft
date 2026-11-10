use crate::tips::Tip as _;
use crate::{Command, Editor, egui};
#[derive(Default)]
pub(super) struct Panel {
    pub active: bool,
    armed: Option<Armed>,
    store: Option<capture_core::icon_library::Library>,
    query: String,
    category: String,
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
    /// Alt+wheel while placing: grows or shrinks the icon about its center.
    pub(super) fn is_armed(&self) -> bool {
        self.armed.is_some()
    }
    pub(super) fn resize_armed(&mut self, steps: i32, canvas: [u32; 2]) -> Option<f32> {
        let payload = &mut self.armed.as_mut()?.payload;
        let a = payload.objects.first_mut()?;
        let size = [a.end[0] - a.start[0], a.end[1] - a.start[1]];
        let longest = size[0].abs().max(size[1].abs()).max(1.);
        let target = (longest * 1.08f32.powi(steps)).clamp(12., largest_fit(a, canvas));
        resize_about_center(a, target / longest);
        reseat(payload);
        Some(target)
    }
}
/// The longest side at which `a` still fits the canvas, as Steps are limited.
fn largest_fit(a: &crate::Annotation, canvas: [u32; 2]) -> f32 {
    let width = (a.end[0] - a.start[0]).abs().max(1.);
    let height = (a.end[1] - a.start[1]).abs().max(1.);
    let ratio = (canvas[0] as f32 / width).min(canvas[1] as f32 / height);
    (width.max(height) * ratio).clamp(12., 16384.)
}
/// A payload is checked against the canvas it came from, so a resized icon
/// carries a canvas of its own size (the reason a large icon could not be placed).
fn reseat(payload: &mut capture_core::clipboard_payload::ObjectPayload) {
    if let Some(a) = payload.objects.first_mut() {
        let size = [(a.end[0] - a.start[0]).abs(), (a.end[1] - a.start[1]).abs()];
        a.start = [0., 0.];
        a.end = size;
        payload.source_canvas = size.map(|side| side.ceil().max(1.) as u32);
    }
}
fn resize_about_center(a: &mut crate::Annotation, ratio: f32) {
    let center = [(a.start[0] + a.end[0]) * 0.5, (a.start[1] + a.end[1]) * 0.5];
    for (axis, middle) in center.into_iter().enumerate() {
        a.start[axis] = middle + (a.start[axis] - middle) * ratio;
        a.end[axis] = middle + (a.end[axis] - middle) * ratio;
    }
}
impl Editor {
    pub(super) fn icon_properties(&mut self, ui: &mut egui::Ui) {
        crate::video_editor::property_design::heading(ui, "Icons", "Icon library");
        if self.icon_library.store.is_none() {
            if let Some(root) = self.history.root() {
                match capture_core::icon_library::Library::open(&root.join("icons")) {
                    Ok(store) => self.icon_library.store = Some(store),
                    Err(e) => self.icon_library.error = e.to_string(),
                }
            } else {
                self.icon_library.error = "Local storage is unavailable.".into();
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Import images…").clicked() {
                self.suspend_global_capture();
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter(
                        "Images",
                        &["png", "jpg", "jpeg", "bmp", "gif", "webp", "svg", "eps"],
                    )
                    .pick_files()
                    && let Some(store) = &mut self.icon_library.store
                {
                    let mut count = 0;
                    for path in paths {
                        match store.import(&path) {
                            Ok(_) => count += 1,
                            Err(e) => {
                                self.icon_library.error = e.to_string();
                                break;
                            }
                        }
                    }
                    self.status = format!("Imported {count} icon(s).");
                }
            }
            if ui.button("Refresh").clicked() {
                if let Some(store) = &mut self.icon_library.store
                    && let Err(e) = store.0.refresh()
                {
                    self.icon_library.error = e.to_string();
                }
                self.icon_library.textures.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Category");
            egui::ComboBox::from_id_salt("icon-category")
                .selected_text(if self.icon_library.category.is_empty() {
                    "All"
                } else {
                    &self.icon_library.category
                })
                .show_ui(ui, |ui| {
                    for name in [
                        "All",
                        "Status",
                        "Notes & alerts",
                        "Workflow",
                        "Input gestures",
                        "Imported",
                        "Samples",
                    ] {
                        ui.selectable_value(
                            &mut self.icon_library.category,
                            if name == "All" {
                                String::new()
                            } else {
                                name.into()
                            },
                            name,
                        );
                    }
                });
        });
        ui.label("Search icons");
        ui.text_edit_singleline(&mut self.icon_library.query);
        if !self.icon_library.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.icon_library.error);
        }
        let mut entries: Vec<Choice> = capture_core::icon_library::builtin::ALL
            .iter()
            .filter(|s| {
                s.matches_query(&self.icon_library.query)
                    && (self.icon_library.category.is_empty()
                        || s.category() == self.icon_library.category)
            })
            .map(|s| Choice {
                id: s.id.into(),
                label: s.label.into(),
            })
            .collect();
        entries.extend(
            self.icon_library
                .store
                .as_ref()
                .map(|s| {
                    s.0.entries()
                        .iter()
                        .filter(|e| {
                            e.matches_query(&self.icon_library.query)
                                && (self.icon_library.category.is_empty()
                                    || self.icon_library.category == "Imported")
                        })
                        .map(|e| Choice {
                            id: e.id.clone(),
                            label: e.label.clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        if entries.is_empty() {
            ui.label("No matching icons.");
        }
        let mut place = None;
        let cell_width = ((ui.available_width() - 12.) / 4.).max(44.);
        egui::Grid::new("icon-grid")
            .num_columns(4)
            .spacing(egui::vec2(4., 8.))
            .show(ui, |ui| {
                for (i, entry) in entries.iter().enumerate() {
                    ui.vertical_centered(|ui| {
                        ui.set_width(cell_width);
                        if !self.icon_library.textures.contains_key(&entry.id)
                            && let Some(pixels) =
                                capture_core::icon_library::builtin::find(&entry.id)
                                    .and_then(|s| s.pixels().ok())
                                    .or_else(|| {
                                        self.icon_library
                                            .store
                                            .as_ref()
                                            .and_then(|s| s.0.thumbnail(&entry.id).ok())
                                    })
                        {
                            let texture = ui.ctx().load_texture(
                                format!("icon-{}", entry.id),
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [pixels.width() as usize, pixels.height() as usize],
                                    pixels.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            );
                            self.icon_library.textures.insert(entry.id.clone(), texture);
                        }
                        if let Some(texture) = self.icon_library.textures.get(&entry.id) {
                            let response = ui
                                .add(
                                    egui::Button::image(
                                        egui::Image::new(texture)
                                            .fit_to_exact_size(egui::vec2(40., 36.)),
                                    )
                                    .selected(
                                        self.icon_library
                                            .armed
                                            .as_ref()
                                            .is_some_and(|a| a.id == entry.id),
                                    )
                                    .min_size(egui::vec2(cell_width, 46.)),
                                )
                                .tip(format!("Place {}", entry.label));
                            if response.clicked() {
                                place = Some(entry.id.clone());
                            }
                        }
                        ui.add(egui::Label::new(&entry.label).truncate())
                            .tip(&entry.label);
                    });
                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
        if let Some(id) = place {
            let result = if let Some(icon) = capture_core::icon_library::builtin::find(&id) {
                icon.payload()
            } else if let Some(store) = &self.icon_library.store {
                store.payload(&id)
            } else {
                return;
            };
            match result {
                Ok(payload) => self.arm_icon(ui.ctx(), id, payload),
                Err(e) => self.icon_library.error = e.to_string(),
            }
        }
    }
    fn arm_icon(
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
        self.icon_library.active = true;
        self.tool = None;
        self.icon_library.placed.clear();
        self.icon_library.anchor = None;
        self.icon_library.armed = Some(Armed {
            id,
            payload,
            preview: None,
            placing: false,
        });
    }

    /// Picking/editing an existing icon establishes the next placement template.
    pub(super) fn remember_placement_template(&mut self, annotation: &crate::Annotation) {
        self.remember_placement_motion(annotation);
        if matches!(annotation.kind, crate::Kind::Pen | crate::Kind::Highlighter) {
            self.remember_annotation_style(annotation);
        }
        if annotation.kind == crate::Kind::Redact {
            self.icon_library.redact = Some(annotation.clone());
        }
        if annotation.kind == crate::Kind::Step {
            self.step_size = (annotation.end[0] - annotation.start[0]).abs().round() as u32;
            self.step_style_size = self.step_size;
            self.step_letters = annotation
                .text
                .as_ref()
                .is_some_and(|t| t.content.chars().any(char::is_alphabetic));
            self.remember_annotation_style(annotation);
            self.icon_library.anchor = Some((crate::Kind::Step, annotation_center(annotation)));
        } else if annotation.image.as_ref().is_some_and(|b| b.is_icon()) {
            let mut a = annotation.clone();
            a.group = None;
            let unchanged = self
                .icon_library
                .armed
                .as_ref()
                .is_some_and(|v| v.payload.objects[0] == a);
            if !unchanged {
                let name = a
                    .image
                    .as_ref()
                    .and_then(|b| b.icon_name())
                    .unwrap_or("Imported icon");
                let id = capture_core::icon_library::builtin::ALL
                    .iter()
                    .find(|s| s.label == name)
                    .map_or_else(|| name.to_owned(), |s| s.id.to_owned());
                self.icon_library.armed = Some(Armed {
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
            self.icon_library.anchor = Some((crate::Kind::Image, annotation_center(annotation)));
        }
    }
    pub(super) fn constrain_placement(
        &self,
        kind: crate::Kind,
        mut point: [f32; 2],
        shift: bool,
    ) -> [f32; 2] {
        if shift
            && let Some((anchor_kind, center)) = self.icon_library.anchor
            && anchor_kind == kind
        {
            let axis = usize::from((point[0] - center[0]).abs() > (point[1] - center[1]).abs());
            point[axis] = center[axis];
        }
        point
    }
    pub(super) fn icon_placement_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: crate::canvas_view::Transform,
        interactive: bool,
    ) -> bool {
        if !self.icon_library.active {
            return false;
        }
        if let Some(a) = self
            .selected_annotation()
            .filter(|a| a.image.as_ref().is_some_and(|b| b.is_icon()))
        {
            self.remember_placement_template(&a);
        }
        if self.icon_library.armed.is_none() {
            let icon = capture_core::icon_library::builtin::find("builtin-check-green").unwrap();
            if let Ok(payload) = icon.payload() {
                self.arm_icon(ui.ctx(), icon.id.into(), payload);
            }
            if self.icon_library.armed.is_none() {
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
            self.icon_library.cancel_drag();
            return true;
        }
        if !interactive || space || self.view.panning {
            return false;
        }
        let visible = transform.image.intersect(transform.viewport);
        // Selected icon_library use the normal move/resize handles. An empty press resumes iconing.
        if pressed
            && let Some(origin) =
                crate::step_tool::placement_origin(ui).filter(|p| visible.contains(*p))
        {
            // A selected icon's handles sit outside its body; a press on one
            // resizes or rotates it and must not place another icon (BJ10).
            let on_handle = self.selected_annotation().is_some_and(|a| {
                a.image.as_ref().is_some_and(|b| b.is_icon())
                    && a.hit_handle(transform.image_point(origin), 7. / transform.scale)
                        .is_some()
            }) || self.rotation_hit(origin, transform);
            if on_handle {
                return false;
            }
            let hit = self
                .session
                .document
                .annotations
                .iter()
                .rev()
                .find(|a| {
                    a.image.as_ref().is_some_and(|b| b.is_icon())
                        && self.session.document.object_editable(a.id)
                        && self.video_object_active(a.id)
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
            self.icon_library.armed.as_mut().unwrap().placing = true;
        }
        if self.edit_drag.is_some()
            || self.rotation_drag.is_some()
            || (!self.icon_library.armed.as_ref().unwrap().placing
                && point.is_some_and(|p| {
                    self.tool_hit(transform.image_point(p), 7. / transform.scale)
                        .is_some()
                        || self.selected_annotation().is_some_and(|a| {
                            a.image.as_ref().is_some_and(|b| b.is_icon())
                                && a.hit_handle(transform.image_point(p), 7. / transform.scale)
                                    .is_some()
                        })
                }))
        {
            return false;
        }
        let Some(point) = point else {
            return true;
        };
        let placing = self.icon_library.armed.as_ref().unwrap().placing;
        let original = self.icon_library.armed.as_ref().unwrap().payload.objects[0].clone();
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
        self.icon_library.placed.retain(|id| {
            self.session
                .document
                .annotations
                .iter()
                .any(|a| a.id == *id)
        });
        for a in self.session.document.annotations.iter().filter(|a| {
            self.icon_library.placed.contains(&a.id) && self.session.document.object_visible(a.id)
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
            let armed = self.icon_library.armed.as_mut().unwrap();
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
            self.icon_library.cancel_drag();
            if visible.contains(point) {
                self.place_icon(center);
            }
        }
        true
    }

    fn place_icon(&mut self, center: [f32; 2]) {
        let Some(armed) = &self.icon_library.armed else {
            return;
        };
        let mut payload = armed.payload.clone();
        let canvas = [self.session.document.width, self.session.document.height];
        if let Some(a) = payload.objects.first_mut() {
            let longest = (a.end[0] - a.start[0])
                .abs()
                .max((a.end[1] - a.start[1]).abs())
                .max(1.);
            let most = largest_fit(a, canvas);
            if longest > most {
                resize_about_center(a, most / longest);
            }
        }
        reseat(&mut payload);
        let before = self.session.document.annotations.len();
        self.execute(Command::PasteObjects { payload, center });
        if self.session.document.annotations.len() <= before {
            return;
        }
        let a = self.session.document.annotations.last().unwrap();
        self.icon_library.placed.push(a.id);
        self.quick_controls.touch_snap(a.id);
        self.icon_library.anchor = Some((crate::Kind::Image, annotation_center(a)));
        self.select_only(None);
        self.status = "Icon placed. Click to place another; select an icon to edit it.".into();
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
                "icon-vector-preview",
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
    fn placement_owns_pointer_over_unlocked_shapes_text_and_raster_images() {
        for kind in [
            crate::Kind::Rectangle,
            crate::Kind::Text,
            crate::Kind::Image,
        ] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            let mut e = crate::canvas::tests::editor(&ctx);
            let mut background = capture_core::steps::marker(
                1,
                [200., 160.],
                160,
                [30, 120, 180, 255],
                (1, false),
                [640, 400],
            )
            .unwrap();
            background.kind = kind;
            if kind != crate::Kind::Text {
                background.text = None;
            }
            if kind == crate::Kind::Image {
                background.image = capture_core::Document::from_capture_image(
                    image::RgbaImage::from_pixel(40, 40, image::Rgba([20, 100, 200, 255])),
                )
                .unwrap()
                .annotations[0]
                    .image
                    .clone();
            }
            e.session.document.annotations = vec![background.clone()];
            e.session.document.promote_features();
            e.session.document.validate().unwrap();
            let icon = &capture_core::icon_library::builtin::ALL[0];
            e.arm_icon(&ctx, icon.id.into(), icon.payload().unwrap());
            assert!(e.tool_hit([200., 160.], 7.).is_none());
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640., 400.));
            let transform = crate::canvas_view::Transform {
                image: area,
                viewport: area,
                scale: 1.,
                zoom: 100.,
            };
            let at = egui::pos2(200., 160.);
            for pressed in [None, Some(true), Some(false)] {
                let mut events = vec![egui::Event::PointerMoved(at)];
                if let Some(pressed) = pressed {
                    events.push(egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                let _ = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(area),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let response = ui.interact(
                            area,
                            egui::Id::unique("icon-over-object"),
                            egui::Sense::click_and_drag(),
                        );
                        assert!(
                            e.icon_placement_input(ui, &response, transform, true),
                            "underlying {kind:?} stole the pointer"
                        );
                    },
                ));
            }
            assert_eq!(
                e.session.document.annotations.len(),
                2,
                "{kind:?}: {}",
                e.status
            );
            assert_eq!(e.session.document.annotations[0], background);
            assert!(
                e.session.document.annotations[1]
                    .image
                    .as_ref()
                    .unwrap()
                    .is_icon()
            );
            assert!(e.icon_library.active && e.selected.is_none());
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations, vec![background]);
        }
    }

    #[test]
    fn selected_icon_edits_seed_repeat_placement_and_vector_preview_cache_tracks_zoom_only() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let icon = capture_core::icon_library::builtin::find("builtin-five-color").unwrap();
        e.arm_icon(&ctx, icon.id.into(), icon.payload().unwrap());
        e.place_icon([110., 110.]);
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
                .named_icon("Five-color workflow"),
        ));
        e.execute(Command::Update {
            annotation: edited.clone(),
        });
        assert_eq!(
            e.icon_library.armed.as_ref().unwrap().payload.objects[0],
            edited
        );
        e.select_only(None);
        e.place_icon([300., 120.]);
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
                        e.icon_library.armed.as_mut().unwrap(),
                        center,
                    );
                },
            ))
        };
        draw(&mut e, 4., [50., 50.]);
        let size = e
            .icon_library
            .armed
            .as_ref()
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .1;
        assert_eq!(size, [360, 360]);
        let moved = draw(&mut e, 4., [80., 65.]);
        assert!(
            moved.textures_delta.set.is_empty(),
            "pointer movement must reuse rendered paths"
        );
        draw(&mut e, 2., [80., 65.]);
        assert_eq!(
            e.icon_library
                .armed
                .as_ref()
                .unwrap()
                .preview
                .as_ref()
                .unwrap()
                .1,
            [180, 180]
        );
    }

    #[test]
    fn video_icon_repeat_inherits_size_and_motion_with_one_undo() {
        use capture_core::video::editor::{Animation, Project};
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session.document.video = Some(Project::new("fixture.mp4".into(), [640, 400], 12000, 30));
        let icon = capture_core::icon_library::builtin::find("builtin-five-color").unwrap();
        e.arm_icon(&ctx, icon.id.into(), icon.payload().unwrap());
        e.place_icon([110., 110.]);
        let mut edited = e.session.document.annotations[0].clone();
        edited.end = [edited.start[0] + 90., edited.start[1] + 90.];
        e.execute(Command::Update {
            annotation: edited.clone(),
        });
        let mut p = e.session.document.video.clone().unwrap();
        let t = p.objects.get_mut(&edited.id).unwrap();
        t.entrance = Animation::Float;
        t.exit = Animation::Fade;
        t.entrance_ms = 750;
        t.exit_ms = 500;
        t.direction = 2;
        p.cursor = 4500;
        e.execute(Command::Video { project: p });
        let before = e.session.document.clone();
        e.place_icon([300., 120.]);
        assert_eq!(e.session.document.annotations.len(), 2, "{}", e.status);
        let a = e.session.document.annotations.last().unwrap();
        let t = &e.session.document.video.as_ref().unwrap().objects[&a.id];
        assert_eq!(
            (t.start, t.entrance_ms, t.exit_ms, t.direction),
            (4500, 750, 500, 2)
        );
        assert_eq!((t.entrance, t.exit), (Animation::Float, Animation::Fade));
        assert_eq!(a.end[0] - a.start[0], 90.);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before.annotations);
        assert_eq!(e.session.document.video, before.video);
    }
    #[test]
    fn icon_pointer_preview_release_repeat_and_reselection_match_step_flow() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let icon = &capture_core::icon_library::builtin::ALL[0];
        e.arm_icon(&ctx, icon.id.into(), icon.payload().unwrap());
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
                        egui::Id::unique("icon-test-canvas"),
                        egui::Sense::click_and_drag(),
                    );
                    e.icon_placement_input(ui, &response, transform, true);
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
        assert!(e.icon_library.active && e.selected.is_none());
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
        // Click a previously placed icon to leave placement and edit its frame.
        let _ = frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(egui::pos2(130., 110.)),
                button(130., 110., true),
            ],
        );
        assert_eq!(e.selected, Some(first));
        assert!(e.icon_library.armed.is_some());
        assert!(e.icon_library.active);
        let _ = frame(&mut e, vec![button(130., 110., false)]);
        // An edited selection remains selected, but empty-canvas hover immediately
        // previews the updated template without requiring a deselection click.
        let mut edited = e.session.document.annotations[0].clone();
        edited.end = [edited.start[0] + 90., edited.start[1] + 90.];
        e.execute(Command::Update { annotation: edited });
        let _ = frame(
            &mut e,
            vec![egui::Event::PointerMoved(egui::pos2(450., 300.))],
        );
        assert_eq!(e.selected, Some(first));
        let armed = e.icon_library.armed.as_ref().unwrap();
        assert_eq!(
            armed.payload.objects[0].end[0] - armed.payload.objects[0].start[0],
            90.
        );
        assert_eq!(armed.preview.as_ref().unwrap().1, [90, 90]);
        // Alt+wheel resizes what is about to be placed, within its limits.
        let canvas = [e.session.document.width, e.session.document.height];
        let grown = e.icon_library.resize_armed(4, canvas).expect("armed icon");
        assert!(grown > 90.);
        assert!(e.icon_library.is_armed());
        let shrunk = e
            .icon_library
            .resize_armed(-200, canvas)
            .expect("armed icon");
        assert_eq!(shrunk, 12.);
        // Grown as far as it goes, it still fits the canvas and can be placed.
        let most = e
            .icon_library
            .resize_armed(400, canvas)
            .expect("armed icon");
        assert!(most <= canvas[0].max(canvas[1]) as f32);
        let before = e.session.document.annotations.len();
        e.place_icon([canvas[0] as f32 * 0.5, canvas[1] as f32 * 0.5]);
        assert_eq!(
            e.session.document.annotations.len(),
            before + 1,
            "{}",
            e.status
        );
        let placed = e.session.document.annotations.last().unwrap();
        assert!(placed.end[0] - placed.start[0] <= canvas[0] as f32 + 0.5);
        assert!(placed.end[1] - placed.start[1] <= canvas[1] as f32 + 0.5);
    }

    #[test]
    fn icon_panel_search_and_placement_use_independent_image_objects() {
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
        let mut store =
            capture_core::icon_library::Library::open(&root.path().join("icons")).unwrap();
        let entry = store.import(&source).unwrap();
        e.icon_library.store = Some(store);
        e.icon_library.query = "Blue badge".into();
        e.icon_library.active = true;
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
            dump_meshes(&ctx, &dir, "Icons", first, output);
        }
        e.icon_library.query.clear();
        e.icon_library.category = "Workflow".into();
        let output = frame(&mut e);
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Time").is_some());
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Green check").is_none());
        e.icon_library.category.clear();
        e.icon_library.query = "no match".into();
        let output = frame(&mut e);
        assert!(
            crate::color_widgets::tests::text_rect(&output.shapes, "No matching icons.").is_some()
        );
        let payload = e
            .icon_library
            .store
            .as_ref()
            .unwrap()
            .payload(&entry.id)
            .unwrap();
        let before = e.session.document.annotations.len();
        e.arm_icon(&ctx, entry.id.clone(), payload);
        assert_eq!(e.session.document.annotations.len(), before);
        assert!(!e.session.can_undo());
        e.place_icon([120., 120.]);
        assert_eq!(e.session.document.annotations.len(), before + 1);
        assert!(e.selected.is_none());
        assert!(e.icon_library.active);
        e.place_icon([300., 200.]);
        assert_eq!(e.session.document.annotations.len(), before + 2);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), before + 1);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), before);
    }
}
