//! One row: left confirmations, centered document controls, right properties tabs.
use crate::{Editor, egui, readiness, shell};
use egui::{Color32, Rect};

#[derive(Clone, Default)]
struct MessageScroll {
    text: String,
    last_time: f64,
    elapsed: f64,
}

#[cfg(test)]
mod tests;

impl Editor {
    pub(super) fn show_footer(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .exact_size(46.)
            .show_separator_line(!self.shell.library)
            .frame(
                egui::Frame::new()
                    .fill(if self.shell.library {
                        shell::SURROUND
                    } else {
                        shell::BAR
                    })
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                let area = ui.available_rect_before_wrap();
                let key = ui
                    .scope_id()
                    .with(("footer-control-width", self.shell.library));
                let measured = ui.ctx().data(|d| d.get_temp::<f32>(key)).unwrap_or(630.);
                // Keep equal space on both sides. Small windows can scroll the
                // controls horizontally, without adding another footer row.
                // Effects/Properties retain their right-hand anchor and never
                // enter the scrolling center group. Reserve matching left space
                // so the other controls remain centered across the whole window.
                let side_space = if self.shell.library { 96. } else { 216. };
                let width = measured.min((area.width() - side_space * 2.).max(1.));
                let controls = Rect::from_center_size(area.center(), egui::vec2(width, 38.));
                let status =
                    Rect::from_min_max(area.min, egui::pos2(controls.left() - 8., area.bottom()));
                let content = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(controls), |ui| {
                        egui::ScrollArea::horizontal()
                            .id_salt("footer-controls")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().scroll.floating = true;
                                ui.horizontal(|ui| self.footer_controls(ui)).response.rect
                            })
                    })
                    .inner;
                let actual = content.inner.width();
                if (actual - measured).abs() > 0.5 {
                    ui.ctx().data_mut(|d| d.insert_temp(key, actual));
                    ui.ctx().request_repaint();
                }
                ui.scope_builder(egui::UiBuilder::new().max_rect(status), |ui| {
                    ui.set_clip_rect(status.intersect(ui.clip_rect()));
                    self.footer_confirmation(ui);
                });
                if !self.shell.library {
                    let tabs = Rect::from_min_size(
                        egui::pos2(area.right() - 208., area.center().y - 17.),
                        egui::vec2(208., 34.),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(tabs), |ui| {
                        self.footer_property_tabs(ui);
                    });
                    #[cfg(test)]
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::unique("footer-tabs"), tabs));
                }
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::unique("footer-layout"),
                        (area, status, controls, content.inner),
                    )
                });
            });
    }

    fn footer_confirmation(&mut self, ui: &mut egui::Ui) {
        let (save, color) = self.autosave_summary();
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
        ui.painter()
            .circle_filled(egui::pos2(rect.left() + 5., rect.center().y), 3., color);
        let message = if self.status.is_empty() {
            save
        } else {
            &self.status
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, message));
        let text_rect = Rect::from_min_max(rect.min + egui::vec2(16., 0.), rect.max);
        let galley = ui.painter().layout_no_wrap(
            message.into(),
            egui::FontId::proportional(12.),
            Color32::from_gray(225),
        );
        let distance = (galley.size().x - text_rect.width()).max(0.);
        let now = ui.input(|i| i.time);
        let key = ui.scope_id().with("message-scroll");
        let mut scroll = ui
            .ctx()
            .data(|d| d.get_temp::<MessageScroll>(key))
            .unwrap_or_default();
        if scroll.text != message {
            scroll.text = message.into();
            scroll.elapsed = 0.;
        } else if !response.hovered() && distance > 0. {
            scroll.elapsed += (now - scroll.last_time).clamp(0., 0.1);
        }
        scroll.last_time = now;
        let cycle = f64::from(distance) / 28. + 3.5;
        let offset =
            (((scroll.elapsed % cycle) - 2.).max(0.) * 28.).min(f64::from(distance)) as f32;
        ui.painter().with_clip_rect(text_rect).galley(
            egui::pos2(
                text_rect.left() - offset,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            Color32::from_gray(225),
        );
        ui.ctx().data_mut(|d| d.insert_temp(key, scroll));
        if distance > 0. && !response.hovered() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("footer-message"),
                (text_rect, offset, distance),
            )
        });
        let response =
            response.on_hover_text(format!("{save}\n{}\nClick for details", self.status));
        egui::Popup::from_toggle_button_response(&response).show(|ui| {
            ui.set_width(320.);
            ui.label(&self.status);
            self.autosave_status(ui);
        });
    }

    fn footer_controls(&mut self, ui: &mut egui::Ui) {
        ui.set_min_height(34.);
        if !self.shell.library {
            let recent = ui
                .button(if self.history.recent && !self.shell.preview_ui {
                    "Hide Recent"
                } else {
                    "Show Recent"
                })
                .on_hover_text(readiness::badge("LIB-001").detail);
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("footer-recent"), recent.rect));
            if recent.clicked() {
                self.history.recent = self.shell.preview_ui || !self.history.recent;
                self.shell.preview_ui = false;
                if self.history.recent {
                    self.history.refresh();
                }
            }
            shell::badge(ui, self.shell.badges, "LIB-008");
            let annotations = ui.button(if self.shell.annotations_hidden { "Show annotations" } else { "Hide annotations" })
                .on_hover_text("Compare the underlying image. This view toggle does not change saved objects or exported images.");
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::unique("footer-annotations"), annotations.rect)
            });
            if annotations.clicked() {
                self.toggle_annotations();
            }
            ui.add_enabled(false, egui::Button::new("Tags"))
                .on_hover_text("Quick-add image tags — planned");
        }
        if self.shell.library {
            self.library_zoom(ui);
            shell::badge(ui, self.shell.badges, "LIB-009");
        } else {
            self.bottom_zoom_controls(ui);
            let dimensions = ui
                .button(format!(
                    "{} × {} px",
                    self.session.document.width, self.session.document.height
                ))
                .on_hover_text("Resize Image");
            self.shell.resize_anchor = Some(dimensions.rect);
            if dimensions.clicked() {
                self.begin_resize();
            }
        }
        crate::controls::toggle(ui, &mut self.shell.badges, "Badges")
            .on_hover_text("Temporary development labels; never included in exported images.");
        if self.shell.library {
            ui.add_enabled(false, egui::Button::new("Export"));
            shell::badge(ui, self.shell.badges, "FILE-006");
        }
    }

    fn footer_property_tabs(&mut self, ui: &mut egui::Ui) {
        crate::tabs::group(ui, |ui| {
            ui.horizontal(|ui| {
                for (label, effects, width) in [("Effects", true, 88.), ("Properties", false, 110.)]
                {
                    let response = crate::tabs::tab(
                        ui,
                        label,
                        self.shell.properties
                            && !self.shell.preview_ui
                            && self.shell.effects == effects,
                        width,
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::unique(("footer-tab", label)), response.rect)
                    });
                    if response.clicked() {
                        self.shell.preview_ui = false;
                        self.shell.properties = true;
                        self.shell.effects = effects;
                    }
                }
            })
        });
    }
}
