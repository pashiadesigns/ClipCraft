use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Choice {
    Rectangle,
    Ellipse,
    Polygon,
    Freehand,
}
impl Choice {
    pub(super) fn is_path(self) -> bool {
        matches!(self, Self::Polygon | Self::Freehand)
    }
    fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Polygon => "Polygon",
            Self::Freehand => "Freehand",
        }
    }
}
impl Editor {
    pub(super) fn set_mask_shape(&mut self, choice: Choice) {
        if let Some(s) = &mut self.pixel_selection {
            s.choice = choice;
            s.shape = if choice == Choice::Ellipse {
                ShapeMask::Ellipse
            } else {
                ShapeMask::Rectangle
            };
            s.region = Crop::empty();
            s.path.clear();
            s.drawing = false;
            s.inverted = false;
            s.pending = None;
        }
    }
    pub(super) fn mask_shape_controls(&mut self, ui: &mut egui::Ui) {
        let Some(s) = &self.pixel_selection else {
            return;
        };
        let mut choice = s.choice;
        let enabled = s.pending.is_none() && !s.dragging();
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt("selection-shape")
                .selected_text(choice.label())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for v in [
                        Choice::Rectangle,
                        Choice::Ellipse,
                        Choice::Polygon,
                        Choice::Freehand,
                    ] {
                        ui.selectable_value(&mut choice, v, v.label());
                    }
                });
        });
        if choice != s.choice {
            self.set_mask_shape(choice);
        }
        let valid = self.selected_mask().is_some();
        if let Some(s) = &mut self.pixel_selection {
            ui.add_enabled_ui(enabled && valid, |ui| {
                crate::controls::toggle(ui, &mut s.inverted, "Inverse selection")
                    .on_hover_text("Select everything outside the mask");
            });
            if s.drawing
                && choice == Choice::Polygon
                && ui
                    .add_enabled(s.path.len() >= 3, egui::Button::new("Close polygon"))
                    .clicked()
            {
                s.finish_path([self.session.document.width, self.session.document.height]);
            }
        }
    }
}
impl PixelSelection {
    fn finish_path(&mut self, size: [u32; 2]) {
        self.drawing = false;
        if self.path.len() < 3 {
            self.path.clear();
            return;
        }
        let min = [0, 1].map(|i| self.path.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min));
        let max = [0, 1].map(|i| {
            self.path
                .iter()
                .map(|p| p[i])
                .fold(f32::NEG_INFINITY, f32::max)
        });
        let Some(b) = PixelRect::from_drag(min, max, size) else {
            self.path.clear();
            return;
        };
        self.shape = ShapeMask::Polygon {
            points: self
                .path
                .iter()
                .map(|p| {
                    [
                        (p[0] - b.x as f32) / b.width as f32,
                        (p[1] - b.y as f32) / b.height as f32,
                    ]
                })
                .collect(),
        };
        self.region.bounds = [b.x, b.y, b.x + b.width, b.y + b.height];
    }
    fn append_point(&mut self, p: [f32; 2]) {
        if self
            .path
            .last()
            .is_some_and(|v| (v[0] - p[0]).hypot(v[1] - p[1]) < 0.5)
        {
            return;
        }
        if self.path.len() >= 1024 {
            self.path = self.path.iter().step_by(2).copied().collect();
        }
        self.path.push(p);
    }
    pub(super) fn interact_shape(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        t: Transform,
        size: [u32; 2],
        interactive: bool,
        panning: bool,
    ) -> bool {
        if !self.choice.is_path() {
            return self
                .region
                .interact(ui, response, t, size, interactive, panning);
        }
        if !interactive {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            return true;
        }
        if !ui.input(|i| i.focused) {
            if self.drawing {
                self.path.clear();
                self.drawing = false;
            }
            return false;
        }
        if panning || ui.input(|i| i.key_down(egui::Key::Space)) {
            return false;
        }
        let point = |p: egui::Pos2| {
            let p = t.image_point(p);
            [
                p[0].clamp(0., size[0] as f32),
                p[1].clamp(0., size[1] as f32),
            ]
        };
        let visible = t.image.intersect(t.viewport);
        let outside = |p: egui::Pos2| {
            let p = t.image_point(p);
            let b = self.region.bounds;
            // Handles extend outside the bounds; preserve their eight-pixel hit area.
            !egui::Rect::from_min_max(
                t.screen_point([b[0] as f32, b[1] as f32]),
                t.screen_point([b[2] as f32, b[3] as f32]),
            )
            .expand(9.)
            .contains(t.screen_point(p))
        };
        let new = if self.choice == Choice::Polygon {
            response.clicked_by(egui::PointerButton::Primary)
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| visible.contains(p) && outside(p))
        } else {
            response.drag_started_by(egui::PointerButton::Primary)
                && ui
                    .input(|i| i.pointer.press_origin())
                    .is_some_and(|p| visible.contains(p) && outside(p))
        };
        if !self.drawing && !self.path.is_empty() && new {
            self.path.clear();
            self.region = Crop::empty();
        }
        if !self.drawing && !self.path.is_empty() {
            return self
                .region
                .interact(ui, response, t, size, interactive, panning);
        }
        if self.choice == Choice::Polygon {
            if response.clicked_by(egui::PointerButton::Primary)
                && let Some(p) = response
                    .interact_pointer_pos()
                    .filter(|p| visible.contains(*p))
            {
                self.drawing = true;
                self.append_point(point(p));
                if response.double_clicked_by(egui::PointerButton::Primary) {
                    self.finish_path(size);
                }
            }
            if self.drawing && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.finish_path(size);
            }
        } else {
            if response.drag_started_by(egui::PointerButton::Primary)
                && let Some(p) = ui
                    .input(|i| i.pointer.press_origin())
                    .filter(|p| visible.contains(*p))
            {
                self.drawing = true;
                self.path.clear();
                self.append_point(point(p));
            }
            if self.drawing
                && let Some(p) = response.interact_pointer_pos()
            {
                self.append_point(point(p));
            }
            if self.drawing && response.drag_stopped_by(egui::PointerButton::Primary) {
                self.finish_path(size);
            }
        }
        if self.drawing {
            ui.ctx().request_repaint();
        }
        false
    }
    pub(super) fn paint_shape(&self, ui: &mut egui::Ui, t: Transform, _size: [u32; 2]) {
        if self.choice == Choice::Rectangle && !self.inverted {
            self.region.paint(ui, t, false);
            return;
        }
        let painter = ui.painter().with_clip_rect(t.image.intersect(t.viewport));
        let b = self.region.bounds;
        let bounds = egui::Rect::from_min_max(
            t.screen_point([b[0] as f32, b[1] as f32]),
            t.screen_point([b[2] as f32, b[3] as f32]),
        );
        let mut points = if self.drawing {
            self.path
                .iter()
                .map(|p| t.screen_point(*p))
                .collect::<Vec<_>>()
        } else {
            match &self.shape {
                ShapeMask::Ellipse => (0..=96)
                    .map(|i| {
                        let a = i as f32 * std::f32::consts::TAU / 96.;
                        bounds.center()
                            + egui::vec2(
                                a.cos() * bounds.width() * 0.5,
                                a.sin() * bounds.height() * 0.5,
                            )
                    })
                    .collect(),
                ShapeMask::Polygon { points } => points
                    .iter()
                    .map(|p| bounds.min + egui::vec2(bounds.width() * p[0], bounds.height() * p[1]))
                    .collect(),
                ShapeMask::Rectangle => vec![
                    bounds.left_top(),
                    bounds.right_top(),
                    bounds.right_bottom(),
                    bounds.left_bottom(),
                ],
            }
        };
        if self.drawing {
            if self.choice == Choice::Polygon
                && let Some(p) = ui.input(|i| i.pointer.hover_pos())
            {
                points.push(p);
            }
        } else if let Some(first) = points.first().copied() {
            points.push(first);
        }
        if points.len() > 1 {
            painter.add(egui::Shape::line(
                points.clone(),
                egui::Stroke::new(2., egui::Color32::BLACK),
            ));
            painter.extend(egui::Shape::dashed_line(
                &points,
                egui::Stroke::new(1., egui::Color32::WHITE),
                4.,
                4.,
            ));
        }
        if !self.drawing && bounds.is_positive() {
            for p in [
                bounds.left_top(),
                bounds.center_top(),
                bounds.right_top(),
                bounds.right_center(),
                bounds.right_bottom(),
                bounds.center_bottom(),
                bounds.left_bottom(),
                bounds.left_center(),
            ] {
                let r = egui::Rect::from_center_size(p, egui::vec2(7., 7.));
                painter.rect_filled(r, 0, egui::Color32::WHITE);
                painter.rect_stroke(
                    r,
                    0,
                    egui::Stroke::new(1., egui::Color32::BLACK),
                    egui::StrokeKind::Inside,
                );
            }
        }
        if self.inverted {
            painter.rect_stroke(
                t.image.shrink(1.),
                0,
                egui::Stroke::new(2., egui::Color32::LIGHT_BLUE),
                egui::StrokeKind::Inside,
            );
        }
    }
}
