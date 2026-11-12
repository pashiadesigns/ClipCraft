//! One row: left confirmations, centered document controls, right properties tabs.
use crate::tips::Tip as _;
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
                // A 38 px band centered top to bottom keeps the buttons centered.
                let controls = Rect::from_center_size(area.center(), egui::vec2(width, 38.));
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
                                // Buttons sit centered top to bottom in the footer.
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| self.footer_controls(ui),
                                )
                                .response
                                .rect
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
                    if self.session.document.part.is_some() && !self.shell.library {
                        self.part_bar(ui);
                    }
                    self.footer_confirmation(ui);
                });
                // Hide UI lives in the footer's reserved right-hand space.
                let hide = Rect::from_center_size(
                    egui::pos2(area.right() - 44., area.center().y),
                    egui::vec2(80., 30.),
                );
                self.show_ui_button(ui, hide);
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
        let now = ui.input(|i| i.time);
        self.sync_notice(now);
        let kind = if self.status.is_empty() {
            crate::guided::Kind::Plain
        } else {
            self.notice.kind
        };
        let since = self.notice.since.unwrap_or(now);
        let (save, color) = self.autosave_summary();
        let save = save.to_owned();
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
        // Errors and warnings fill the slot in their color; an error flashes
        // twice when it appears (approved message bar).
        if matches!(
            kind,
            crate::guided::Kind::Error | crate::guided::Kind::Warning
        ) {
            let base = if kind == crate::guided::Kind::Error {
                crate::guided::RED
            } else {
                crate::guided::AMBER
            };
            let t = (now - since) as f32;
            let flash = if kind == crate::guided::Kind::Error
                && t < 2.4
                && ui.ctx().global_style().animation_time > 0.
            {
                ui.ctx().request_repaint();
                1. - (t % 1.2) / 1.2
            } else {
                0.
            };
            let slot = rect.shrink2(egui::vec2(0., 4.));
            ui.painter()
                .rect_filled(slot, 6., base.gamma_multiply(0.16 + 0.3 * flash));
            ui.painter().rect_filled(
                Rect::from_min_size(slot.min, egui::vec2(3., slot.height())),
                egui::CornerRadius {
                    nw: 6,
                    sw: 6,
                    ne: 0,
                    se: 0,
                },
                base,
            );
        }
        let dot = egui::pos2(rect.left() + 13., rect.center().y);
        if kind == crate::guided::Kind::Plain {
            ui.painter().circle_filled(dot, 3., color);
        } else {
            crate::guided::icon(ui, dot, kind, since);
        }
        let message = if self.status.is_empty() {
            save.as_str()
        } else {
            self.status.as_str()
        };
        let message = message.to_owned();
        let message = message.as_str();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, message));
        let text_rect = Rect::from_min_max(rect.min + egui::vec2(26., 0.), rect.max);
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
        if matches!(
            kind,
            crate::guided::Kind::Error | crate::guided::Kind::Warning
        ) {
            // The card above the bar: the whole message and its fix. It stays
            // until closed; a click elsewhere hides it and the bar keeps its
            // color; clicking the bar brings it back.
            if response.clicked() {
                self.notice.card = !self.notice.card;
            }
            // The card never covers the fix (user, 2026-11-12): it tucks into
            // the bar once a control is ringed, and waits while a dialog is
            // open (the dialog shows its own message).
            if self.notice.card && crate::guided::active(ui.ctx()).is_some() {
                self.notice.card = false;
            }
            let dialog_open = ui.ctx().memory(|m| m.top_modal_layer()).is_some();
            if self.notice.card && !dialog_open {
                self.message_card(ui, rect, kind);
            }
            return;
        }
        // A finished action that can be undone shows its link at the end.
        if kind == crate::guided::Kind::Done
            && let Some((label, action)) = self.notice.action.clone()
        {
            let at = Rect::from_min_size(
                egui::pos2(rect.right() - 64., rect.center().y - 11.),
                egui::vec2(60., 22.),
            );
            let link = ui.put(
                at,
                egui::Button::new(
                    egui::RichText::new(label).color(Color32::from_rgb(127, 208, 239)),
                )
                .frame(false),
            );
            if link.clicked() {
                let ctx = ui.ctx().clone();
                self.notice.action = None;
                self.run_fix(action, &ctx);
            }
        }
        let response = response.tip(format!("{save}\n{}\nClick for details", self.status));
        egui::Popup::from_toggle_button_response(&response).show(|ui| {
            ui.set_width(320.);
            ui.label(&self.status);
            self.autosave_status(ui);
        });
    }

    fn message_card(&mut self, ui: &mut egui::Ui, bar: Rect, kind: crate::guided::Kind) {
        let color = if kind == crate::guided::Kind::Error {
            crate::guided::RED
        } else {
            crate::guided::AMBER
        };
        let mut close = false;
        let mut run = None;
        let area = egui::Area::new(egui::Id::unique("message-card"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(egui::pos2(bar.left(), bar.top() - 6.))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(52, 59, 62))
                    .stroke(egui::Stroke::new(1., color))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.set_max_width(380.);
                        ui.add(
                            egui::Label::new(egui::RichText::new(&self.status).strong().size(13.5))
                                .wrap(),
                        );
                        ui.add_space(6.);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some((label, action)) = &self.notice.action
                                && ui
                                    .add(crate::controls::dialog_button(label, true))
                                    .clicked()
                            {
                                run = Some(action.clone());
                            }
                            if ui
                                .add(crate::controls::dialog_button("Close", false))
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
            });
        // A click anywhere else (the canvas, say) hides the card.
        let card = area.response.rect;
        if ui.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !card.contains(p) && !bar.contains(p))
        }) {
            close = true;
        }
        if close {
            self.notice.card = false;
        }
        if let Some(action) = run {
            self.notice.card = false;
            let ctx = ui.ctx().clone();
            self.run_fix(action, &ctx);
        }
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
                .tip("Show or hide recent clips");
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
            let annotations=icon_caption_button(ui,annotations_label,"").tip(format!("{annotations_label} · compare the underlying image without changing saved objects or exports."));
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
                .tip("Resize Image");
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

impl Editor {
    /// "Part 3 of 10 · ‹ Previous · Next ›" for a linked part (approved beta 6
    /// tab 7); the other parts open from the Library.
    fn part_bar(&mut self, ui: &mut egui::Ui) {
        use crate::tips::Tip as _;
        let Some(part) = self.session.document.part.clone() else {
            return;
        };
        let siblings = self.part_siblings(&part);
        let find = |index: u32| {
            siblings
                .iter()
                .find(|e| e.part.as_ref().is_some_and(|p| p.index == index))
                .map(|e| e.id.clone())
        };
        let previous = (part.index > 1).then(|| find(part.index - 1)).flatten();
        let next = (part.index < part.total)
            .then(|| find(part.index + 1))
            .flatten();
        let mut open = None;
        ui.horizontal_centered(|ui| {
            if ui
                .add_enabled(previous.is_some(), egui::Button::new("‹").small())
                .tip("Previous part · Open the part before this one.")
                .clicked()
            {
                open = previous.clone();
            }
            ui.label(
                egui::RichText::new(format!("Part {} of {}", part.index, part.total))
                    .size(12.)
                    .color(egui::Color32::from_rgb(200, 214, 219)),
            )
            .tip(format!(
                "{} · Linked parts of one PDF. Export can put every part into one file.",
                part.label()
            ));
            if ui
                .add_enabled(next.is_some(), egui::Button::new("›").small())
                .tip("Next part · Open the part after this one.")
                .clicked()
            {
                open = next.clone();
            }
        });
        if let Some(id) = open {
            let ctx = ui.ctx().clone();
            self.open_history(&id, &ctx);
        }
    }
}
