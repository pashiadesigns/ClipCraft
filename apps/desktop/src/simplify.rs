use crate::{Editor, Kind, egui, property_layout};

#[cfg(test)]
mod tests;

impl Editor {
    pub(super) fn analyze_simplify(&mut self) -> Vec<crate::Annotation> {
        match self.session.document.render() {
            Ok(image) => capture_core::simplify::analyze(&image, self.simplify_detail)
                .into_iter()
                .enumerate()
                .map(|(i, b)| {
                    let mut a = self.new_annotation(Kind::Simplify, b.start, b.end);
                    a.id = self.session.next_id() + i as u64;
                    a.color = b.color;
                    a.corner_radius = 0.;
                    a.shadow = None;
                    a.rotation_degrees = 0.;
                    a
                })
                .collect(),
            Err(error) => {
                self.status = error.to_string();
                vec![]
            }
        }
    }
    pub(super) fn commit_simplify(
        &mut self,
        mut annotations: Vec<crate::Annotation>,
        bounds: ([f32; 2], [f32; 2]),
    ) {
        let count = annotations.len();
        if count == 0 {
            self.status =
                "No distinct details found. Increase Detail or use a manual block.".into();
            return;
        }
        let group = self
            .session
            .document
            .annotations
            .iter()
            .filter_map(|a| a.group)
            .max()
            .unwrap_or(0)
            + 1;
        let first = annotations[0].id;
        for a in &mut annotations {
            a.group = Some(group);
        }
        let mut anchor = self.new_annotation(Kind::Simplify, bounds.0, bounds.1);
        anchor.id = annotations.iter().map(|a| a.id).max().unwrap() + 1;
        anchor.group = Some(group);
        anchor.analysis = Some(Box::new(capture_core::simplify::Analysis {
            detail: self.simplify_detail,
            colors: annotations
                .iter()
                .map(|a| a.color)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        }));
        anchor.color = [0; 4];
        anchor.corner_radius = 0.;
        anchor.shadow = None;
        anchor.rotation_degrees = 0.;
        annotations.insert(0, anchor);
        self.execute(crate::Command::AddMany { annotations });
        self.editing_group = None;
        self.select_only(Some(first));
        self.status =
            format!("Simplified {count} regions into editable blocks. Undo restores the image.");
    }
    pub(super) fn refresh_simplify_palette(&mut self) {
        match self.session.document.render() {
            Ok(image) => self.simplify_palette = capture_core::simplify::palette(&image),
            Err(error) => self.status = error.to_string(),
        }
    }
    pub(super) fn simplify_properties(&mut self, ui: &mut egui::Ui) {
        let selected = self
            .selected_annotation()
            .filter(|a| a.kind == Kind::Simplify);
        let mut edited = selected
            .clone()
            .unwrap_or_else(|| self.new_annotation(Kind::Simplify, [0., 0.], [200., 100.]));
        let before = edited.clone();
        ui.horizontal(|ui| property_layout::swatch(ui, "Color", &mut edited.color));
        if !self.simplify_palette.is_empty() {
            ui.horizontal(|ui| {
                for color in &self.simplify_palette {
                    let response = ui
                        .add_sized(
                            [26., 24.],
                            egui::Button::new("").fill(egui::Color32::from_rgba_unmultiplied(
                                color[0], color[1], color[2], color[3],
                            )),
                        )
                        .on_hover_text(format!(
                            "Image color #{:02X}{:02X}{:02X}",
                            color[0], color[1], color[2]
                        ));
                    if response.clicked() {
                        edited.color = *color;
                    }
                }
            });
        }
        if ui.button("Sample image colors").clicked() {
            self.refresh_simplify_palette();
        }
        if selected.is_none() {
            crate::controls::toggle(ui, &mut self.simplify_auto, "Analyze drawn area");
            property_layout::slider(
                ui,
                "Detail",
                egui::Slider::new(&mut self.simplify_detail, 0..=100),
            );
            if ui.button("Auto Simplify image").on_hover_text("Detect text and interface regions, then add editable blocks in their sampled colors.").clicked() {
                let blocks = self.analyze_simplify();
                self.commit_simplify(blocks, ([0., 0.], [self.session.document.width as f32, self.session.document.height as f32]));
            }
        }
        property_layout::slider(
            ui,
            "Opacity",
            egui::Slider::new(&mut edited.color[3], 0..=255),
        );
        let max = edited.max_corner_radius();
        property_layout::slider(
            ui,
            "Corners",
            egui::Slider::new(&mut edited.corner_radius, 0.0..=max),
        );
        crate::shadow_properties::show(ui, &mut edited);
        if selected.is_some() {
            let rotation = property_layout::rotation_controls(ui, &mut edited)
                .is_some_and(|r| r[0].changed() || r[1].clicked());
            if edited != before || rotation {
                self.preview_property_edit_with_rotation(edited, rotation);
            }
        } else if edited != before {
            self.remember_annotation_style(&edited);
        }
    }
}
