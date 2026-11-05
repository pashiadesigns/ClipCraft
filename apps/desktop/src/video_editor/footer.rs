//! Video-only controls share the centered, single-row application footer.
use super::*;
use crate::canvas_view::{ZoomAction, ZoomMode};

fn control(
    ui: &mut egui::Ui,
    key: &str,
    glyph: &str,
    selected: bool,
    hint: &str,
) -> egui::Response {
    let response = ui
        .push_id(key, |ui| super::ui::button(ui, glyph, "", 30., selected))
        .inner;
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(key), response.rect));
    response.on_hover_text(hint)
}

fn divider(ui: &mut egui::Ui) {
    ui.add_space(5.);
    ui.separator();
    ui.add_space(5.);
}

impl Editor {
    pub(crate) fn open_video_setup(&mut self) {
        self.video.settings = true;
        self.video.tab = PropertyTab::Style;
        self.video.object_tab = PropertyTab::Style;
        self.select_only(None);
        self.video.selected = Selection::Objects;
        self.tool = None;
        self.shell.properties = true;
        self.shell.preview_ui = false;
    }

    pub(crate) fn video_footer_controls(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = 5.;
        let recent = self.history.recent && !self.shell.preview_ui;
        if control(
            ui,
            "footer-recent",
            "Recent Clips",
            recent,
            "Show/Hide Recent Clips",
        )
        .clicked()
        {
            self.set_recent_visible(!recent);
            self.shell.preview_ui = false;
            if self.history.recent {
                self.history.refresh();
            }
        }
        let timeline = !self.video.collapsed && !self.shell.preview_ui;
        if control(
            ui,
            "footer-timeline",
            "Timeline",
            timeline,
            "Show/Hide Timeline · opening it hides Recent Clips",
        )
        .clicked()
        {
            self.set_timeline_expanded(!timeline);
            self.shell.preview_ui = false;
        }
        let annotations = if self.shell.annotations_hidden {
            "Show annotations"
        } else {
            "Hide annotations"
        };
        if control(
            ui,
            "footer-annotations",
            annotations,
            self.shell.annotations_hidden,
            "Show/Hide annotations · saved objects and exports stay unchanged",
        )
        .clicked()
        {
            self.toggle_annotations();
        }
        let priority = self.video.properties_priority;
        if control(
            ui,
            "footer-priority",
            if priority {
                "Expand Timeline"
            } else {
                "Expand Properties pane"
            },
            priority,
            if priority {
                "Expand Timeline · Properties docks above it"
            } else {
                "Expand Properties pane · timeline docks beside it"
            },
        )
        .clicked()
        {
            self.video.properties_priority = !priority;
            self.shell.properties = true;
            self.shell.preview_ui = false;
        }
        divider(ui);
        if control(
            ui,
            "footer-snap",
            "Timeline snapping",
            self.video.snap,
            "Timeline snapping",
        )
        .clicked()
        {
            self.video.snap = !self.video.snap;
        }
        divider(ui);
        let mut action = None;
        ui.add_enabled_ui(self.text_draft.is_none(), |ui| {
            for (key, glyph, choice) in [
                ("footer-zoom-out", "Zoom preview out", ZoomAction::Out),
                ("footer-zoom-in", "Zoom preview in", ZoomAction::In),
                ("footer-fit", "Fit preview", ZoomAction::Set(ZoomMode::Fit)),
            ] {
                if control(ui, key, glyph, false, glyph).clicked() {
                    action = Some(choice);
                }
            }
            let response = super::ui::button(ui, "Down", &self.view.label(), 76., false)
                .on_hover_text(
                    "Canvas zoom · Configured zoom modifier + wheel to zoom · middle-drag to pan",
                );
            egui::Popup::menu(&response).show(|ui| {
                for percent in [10, 25, 50, 75, 100, 125, 150, 200, 300, 400, 800, 1600] {
                    let mode = ZoomMode::Manual(percent as f32 / 100.);
                    if ui
                        .selectable_label(self.view.mode == mode, format!("{percent}%"))
                        .clicked()
                    {
                        action = Some(ZoomAction::Set(mode));
                        ui.close();
                    }
                }
            });
        });
        if let Some(action) = action {
            let crop = self.crop.take();
            self.cancel_gesture();
            self.crop = crop;
            self.view.apply(action, None);
            ui.ctx().request_repaint();
        }
        let dimensions = format!(
            "{} × {}",
            self.session.document.width, self.session.document.height
        );
        let response = super::ui::button(ui, "Video Setup", &dimensions, 130., false)
            .on_hover_text("Video Setup · resolution and frame rate");
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("footer-video-setup"), response.rect));
        if response.clicked() {
            self.open_video_setup();
        }
    }
}

impl Editor {
    pub(super) fn add_track_menu(&mut self, ui: &mut egui::Ui) {
        use capture_core::video::editor::TrackKind;
        for (label, glyph, kind) in [
            ("Annotation track", "Tracks", None),
            ("Audio track", "Audio", Some(TrackKind::Audio)),
            ("Voiceover track", "Voiceover", Some(TrackKind::Voiceover)),
            ("Video track", "Recording", Some(TrackKind::Video)),
        ] {
            let p = self.session.document.video.as_ref().unwrap();
            let enabled = if kind.is_some() {
                p.media_tracks.len() < 64
            } else {
                p.tracks.len() < 1000
            };
            let response = ui
                .add_enabled_ui(enabled, |ui| {
                    super::ui::button(ui, glyph, label, 190., false)
                })
                .inner;
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(("add-track", label)), response.rect));
            #[cfg(test)]
            if kind.is_none() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique("add-object-track"), response.rect)
                });
            }
            if response.clicked() {
                if kind == Some(TrackKind::Voiceover) && self.video.voiceover.open {
                    self.open_voiceover();
                    ui.close();
                    continue;
                }
                let mut p = self.session.document.video.clone().unwrap();
                let track = if let Some(kind) = kind {
                    p.add_typed_track(kind)
                } else {
                    let id = p.add_track();
                    p.annotation_track = Some(id);
                    id
                };
                self.commit_video(p);
                self.select_only(None);
                self.video.selected = Selection::Objects;
                self.set_timeline_expanded(true);
                if kind == Some(TrackKind::Voiceover) {
                    self.open_voiceover();
                    self.video.voiceover.target = Some(track);
                }
                self.status = format!(
                    "{label} added. {}",
                    if kind == Some(TrackKind::Voiceover) {
                        "Record your take in the voiceover window."
                    } else if kind.is_some() {
                        "Drop a file onto its timeline row or right-click the track to import."
                    } else {
                        "Move annotation blocks here using their track menu or drag them onto the row."
                    }
                );
                ui.close();
            }
        }
    }
}
