//! One row: left confirmations, centered document controls, right properties tabs.
use crate::{Editor, egui, shell};
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
            .exact_size(56.)
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
                let key = ui.scope_id().with((
                    "footer-control-width",
                    self.shell.library,
                    self.is_video(),
                ));
                let measured = ui.ctx().data(|d| d.get_temp::<f32>(key)).unwrap_or(630.);
                // Keep equal space on both sides. Small windows can scroll the
                // controls horizontally, without adding another footer row.
                // Effects/Properties retain their right-hand anchor and never
                // enter the scrolling center group. Reserve matching left space
                // so the other controls remain centered across the whole window.
                let side_space = 96.;
                let width = measured.min((area.width() - side_space * 2.).max(1.));
                let controls = Rect::from_center_size(area.center(), egui::vec2(width, 48.));
                let status =
                    Rect::from_min_max(area.min, egui::pos2(controls.left() - 8., area.bottom()));
                let content = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(controls), |ui| {
                        egui::ScrollArea::horizontal()
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                            )
                            .id_salt("footer-controls")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
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
        if self.is_video() && !self.shell.library {
            self.video_footer_controls(ui);
            return;
        }
        if !self.shell.library {
            let recent_label = if self.history.recent && !self.shell.preview_ui {
                "Hide Recent Clips"
            } else {
                "Show Recent Clips"
            };
            let recent = icon_caption_button(ui, recent_label, recent_label)
                .on_hover_text("Show or hide recent clips");
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("footer-recent"), recent.rect));
            if recent.clicked() {
                self.set_recent_visible(self.shell.preview_ui || !self.history.recent);
                self.shell.preview_ui = false;
                if self.history.recent {
                    self.history.refresh();
                }
            }

            let annotations_label = if self.shell.annotations_hidden {
                "Show annotations"
            } else {
                "Hide annotations"
            };
            let annotations=icon_caption_button(ui,annotations_label,"").on_hover_text(format!("{annotations_label} · compare the underlying image without changing saved objects or exports."));
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::unique("footer-annotations"), annotations.rect)
            });
            if annotations.clicked() {
                self.toggle_annotations();
            }
        }
        if self.shell.library {
            self.library_zoom(ui);
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
    }
}

fn icon_caption_button(ui: &mut egui::Ui, icon: &str, label: &str) -> egui::Response {
    let font = egui::FontId::proportional(12.);
    let text = ui
        .painter()
        .layout_no_wrap(label.into(), font, ui.visuals().text_color());
    let width = if label.is_empty() {
        28.
    } else {
        text.size().x + 34.
    };
    let response = ui.add_sized([width, 26.], egui::Button::new(""));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), icon));
    let center = response.rect.center();
    let icon_center = if label.is_empty() {
        center
    } else {
        egui::pos2(response.rect.left() + 15., center.y)
    };
    crate::icons::draw_scaled(
        ui.painter(),
        egui::Rect::from_center_size(icon_center, egui::vec2(20., 20.)),
        icon,
        ui.visuals().text_color(),
        0.75,
    );
    if !label.is_empty() {
        ui.painter().galley(
            egui::pos2(response.rect.left() + 28., center.y - text.size().y * 0.5),
            text,
            ui.visuals().text_color(),
        );
    }
    response
}
