//! Original reorderable toolbar. Tool IDs stay independent of visible positions.
use crate::{
    Editor, egui,
    shell::{TOOLS, badge, tool_button},
};
use capture_core::{
    quick_preferences::Choice,
    toolbar::{Group, ToolId},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
struct ToolDrag(ToolId);

fn before_at(order: &[ToolId], id: ToolId, direction: isize) -> Option<ToolId> {
    let index = order.iter().position(|t| *t == id).unwrap();
    if direction < 0 {
        Some(order[index.saturating_sub(1)])
    } else {
        order.get(index + 2).copied()
    }
}

impl Editor {
    pub(super) fn apply_toolbar_choice(&mut self, choice: Choice) {
        let choice = match choice {
            Choice::MoveTool { tool, before } => Choice::MoveSectionTool { tool, before },
            other => other,
        };
        match self.quick_controls.change_toolbar(choice) {
            Ok(true) => self.status = "Tool sections and favorites saved.".into(),
            Ok(false) => {}
            Err(error) => self.status = format!("Toolbar order could not be saved: {error}"),
        }
    }

    #[cfg(test)]
    pub(super) fn toolbar_buttons(
        &mut self,
        ui: &mut egui::Ui,
        visible: usize,
    ) -> Vec<(ToolId, egui::Response)> {
        self.toolbar_range(ui, 0, visible)
    }

    pub(super) fn toolbar_range(
        &mut self,
        ui: &mut egui::Ui,
        start: usize,
        visible: usize,
    ) -> Vec<(ToolId, egui::Response)> {
        if !ui.input(|i| i.focused) {
            egui::DragAndDrop::clear_payload(ui.ctx());
        }
        let order = self.quick_controls.toolbar_order().to_vec();
        let visible = visible.min(order.len());
        let current = self.current_tool().id;
        let mut responses = Vec::new();
        // Defer preference changes until every slot in this frame has been laid
        // out. A release never shifts hit targets midway through the row.
        let mut reorder = None;
        for (slot, id) in order.iter().copied().take(visible).enumerate().skip(start) {
            let active_id = if id == ToolId::Crop {
                self.quick_controls.canvas_tool()
            } else {
                id
            };
            let index = TOOLS.iter().position(|t| t.id == active_id).unwrap();
            let response = ui
                .push_id(("toolbar-tool", id), |ui| {
                    crate::shell::tool_button_label(
                        ui,
                        TOOLS[index],
                        if id == ToolId::Crop {
                            "Crop / Cut"
                        } else {
                            TOOLS[index].name
                        },
                        current == active_id,
                        self.shell.badges,
                    )
                })
                .inner
                .on_hover_text(self.tool_tooltip(active_id));
            response.dnd_set_drag_payload(ToolDrag(id));
            if let Some(payload) = response.dnd_hover_payload::<ToolDrag>().filter(|p| {
                self.quick_controls.tool_group(p.0) == self.quick_controls.tool_group(id)
            }) {
                let mut after = ui.input(|i| {
                    i.pointer
                        .hover_pos()
                        .is_some_and(|p| p.x > response.rect.center().x)
                });
                // Promoting a hidden tool must leave it visibly on the bar,
                // including a drop at the right edge of the last visible slot.
                if !order[..visible].contains(&payload.0) && slot + usize::from(after) >= visible {
                    after = false;
                }
                let before = if after {
                    order.get(slot + 1).copied().filter(|t| {
                        self.quick_controls.tool_group(*t) == self.quick_controls.tool_group(id)
                    })
                } else {
                    Some(id)
                };
                let x = if after {
                    response.rect.right()
                } else {
                    response.rect.left()
                };
                ui.painter().line_segment(
                    [
                        egui::pos2(x, response.rect.top()),
                        egui::pos2(x, response.rect.bottom()),
                    ],
                    egui::Stroke::new(3.0, egui::Color32::from_rgb(65, 190, 240)),
                );
                if let Some(payload) = response.dnd_release_payload::<ToolDrag>() {
                    reorder = Some(Choice::MoveSectionTool {
                        tool: payload.0,
                        before,
                    });
                }
            }
            let group_order: Vec<_> = order
                .iter()
                .copied()
                .filter(|t| {
                    self.quick_controls.tool_group(*t) == self.quick_controls.tool_group(id)
                })
                .collect();
            if let Some(choice) =
                self.toolbar_reorder_controls(ui, &response, id, &group_order, false)
            {
                reorder = Some(choice);
            }
            if id == ToolId::Crop {
                egui::Popup::menu(&response).show(|ui| self.canvas_tool_choices(ui));
            } else if response.clicked() {
                self.choose_tool(index);
            }
            responses.push((id, response));
        }
        if let Some(choice) = reorder {
            self.apply_toolbar_choice(choice);
        }
        responses
    }

    fn canvas_tool_choices(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            for id in [ToolId::Crop, ToolId::CutOut] {
                let index = TOOLS.iter().position(|t| t.id == id).unwrap();
                let response = ui
                    .push_id(id, |ui| {
                        tool_button(
                            ui,
                            TOOLS[index],
                            self.quick_controls.canvas_tool() == id,
                            false,
                        )
                    })
                    .inner
                    .on_hover_text(self.tool_tooltip(id));
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique(("canvas-tool-choice", id)), response.rect)
                });
                if response.clicked() {
                    self.choose_tool(index);
                    ui.close();
                }
            }
        });
    }

    fn toolbar_reorder_controls(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        id: ToolId,
        order: &[ToolId],
        nested: bool,
    ) -> Option<Choice> {
        let mut choice = None;
        let content = |ui: &mut egui::Ui| {
            let favorite = self.quick_controls.tool_group(id) == Group::Favorites;
            let r = ui.button(if favorite {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            });
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("favorite-context-action"), r.rect));
            if r.clicked() {
                choice = Some(Choice::FavoriteTool {
                    tool: id,
                    favorite: !favorite,
                });
                ui.close();
            }
            ui.separator();
            for (label, before) in [
                ("Move to section front", order.first().copied()),
                ("Move left", before_at(order, id, -1)),
                ("Move right", before_at(order, id, 1)),
                ("Move to end", None),
            ] {
                if ui.button(label).clicked() {
                    choice = Some(Choice::MoveSectionTool { tool: id, before });
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Reset toolbar order").clicked() {
                choice = Some(Choice::ResetToolbar);
                ui.close();
            }
        };
        if nested {
            let key = egui::Id::unique("overflow-tool-context");
            if response.secondary_clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(key, id));
                egui::containers::menu::MenuState::from_ui(ui, |state, _| state.open_item = None);
            }
            if ui.ctx().data(|d| d.get_temp::<ToolId>(key)) == Some(id) {
                egui::containers::menu::SubMenu::new().show(ui, response, content);
            }
        } else {
            response.context_menu(content);
        }
        if response.has_focus() {
            let before = ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft) {
                    Some(before_at(order, id, -1))
                } else if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight) {
                    Some(before_at(order, id, 1))
                } else if i.consume_key(egui::Modifiers::ALT, egui::Key::Home) {
                    Some(order.first().copied())
                } else {
                    None
                }
            });
            if let Some(before) = before {
                choice = Some(Choice::MoveSectionTool { tool: id, before });
            }
        }
        choice
    }

    pub(super) fn draggable_tools_menu(
        &mut self,
        ui: &mut egui::Ui,
        visible: usize,
    ) -> Vec<(ToolId, egui::Response)> {
        let order = self.quick_controls.toolbar_order().to_vec();
        let pass = ui.ctx().cumulative_pass_nr();
        ui.ctx().data_mut(|d| {
            let last = d.get_temp::<u64>(egui::Id::unique("overflow-tool-pass"));
            if last.is_none_or(|last| last + 1 < pass) {
                d.remove::<ToolId>(egui::Id::unique("overflow-tool-context"));
            }
            d.insert_temp(egui::Id::unique("overflow-tool-pass"), pass);
        });
        let mut responses = Vec::new();
        let mut reorder = None;
        egui::ScrollArea::vertical()
            .max_height(520.0)
            .show(ui, |ui| {
                ui.label("Right-click to favorite. Drag within the same section.");
                let mut last_group = None;
                for id in order.iter().copied().skip(visible) {
                    let group = self.quick_controls.tool_group(id);
                    if last_group != Some(group) {
                        if last_group.is_some() {
                            ui.separator();
                        }
                        ui.strong(group.label());
                        last_group = Some(group);
                    }
                    let active_id = if id == ToolId::Crop {
                        self.quick_controls.canvas_tool()
                    } else {
                        id
                    };
                    let index = TOOLS.iter().position(|t| t.id == active_id).unwrap();
                    let tool = TOOLS[index];
                    let response = ui
                        .push_id(("more-tool", id), |ui| {
                            ui.horizontal(|ui| {
                                let response = ui.add(
                                    egui::Button::new(if id == ToolId::Crop {
                                        "Crop / Cut"
                                    } else {
                                        tool.name
                                    })
                                    .frame(false)
                                    .min_size(egui::vec2(150.0, 22.0))
                                    .sense(egui::Sense::click_and_drag()),
                                );
                                badge(ui, self.shell.badges, tool.feature);
                                response
                            })
                            .inner
                        })
                        .inner
                        .on_hover_text(self.tool_tooltip(active_id));
                    response.dnd_set_drag_payload(ToolDrag(id));
                    let group_order: Vec<_> = order
                        .iter()
                        .copied()
                        .filter(|t| self.quick_controls.tool_group(*t) == group)
                        .collect();
                    if let Some(choice) =
                        self.toolbar_reorder_controls(ui, &response, id, &group_order, true)
                    {
                        reorder = Some(choice);
                    }
                    if id == ToolId::Crop
                        && ui.ctx().data(|d| {
                            d.get_temp::<ToolId>(egui::Id::unique("overflow-tool-context"))
                        }) != Some(id)
                    {
                        egui::containers::menu::SubMenu::new()
                            .show(ui, &response, |ui| self.canvas_tool_choices(ui));
                    } else if response.clicked() {
                        self.choose_tool(index);
                        ui.close();
                    }
                    responses.push((id, response));
                }
                ui.separator();
                if ui.button("Reset toolbar order").clicked() {
                    reorder = Some(Choice::ResetToolbar);
                    ui.close();
                }
            });
        if let Some(choice) = reorder {
            self.apply_toolbar_choice(choice);
        }
        responses
    }
}
