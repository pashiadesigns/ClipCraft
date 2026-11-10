use crate::{Command, Editor, egui};
use capture_core::{group_presets::Composition, reusable_presets::Payload};
impl Editor {
    pub(crate) fn save_group_preset(&mut self) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        let ids = self.group_preset_ids();
        match Composition::capture(&self.session.document, &ids) {
            Ok(c) => self.presets.reusable.save_payload(Payload::Group(c)),
            Err(e) => self.status = e.to_string(),
        }
    }
    /// Objects a saved group captures: the canvas selection, or the objects
    /// selected on the video timeline when the timeline holds the multi-selection.
    pub(crate) fn group_preset_ids(&self) -> Vec<u64> {
        let canvas = self.selection_ordered_ids();
        if canvas.len() > 1 {
            return canvas;
        }
        let timeline = self.timeline_selected_objects();
        if timeline.len() > 1 { timeline } else { canvas }
    }
    /// Width the Blocks group takes, its leading divider included.
    pub(crate) const GROUP_PRESET_WIDTH: f32 = 180.;
    /// The Blocks group: always the last group of the Insert tab, in image and
    /// video mode, after a divider.
    pub(crate) fn group_preset_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        crate::ribbon::divider(ui, height);
        crate::ribbon::group(ui, "Presets", 160., height, |ui| {
            if crate::ribbon::command_tile(
                ui,
                "Group presets",
                "Blocks",
                "Insert a saved block of editable objects",
                true,
                false,
                76.,
                height - 14.,
            )
            .clicked()
            {
                self.presets.reusable.open_groups();
            }
            if crate::ribbon::command_tile(
                ui,
                "Save group",
                "Save as block",
                "Save the selected objects as a reusable block",
                self.group_preset_ids().len() > 1,
                false,
                76.,
                height - 14.,
            )
            .clicked()
            {
                self.save_group_preset();
            }
        });
    }
    pub(crate) fn place_group_preset(&mut self) {
        let Some(preset) = self.presets.reusable.take_group() else {
            return;
        };
        if !self.commit_text() {
            return;
        }
        let center = [
            self.session.document.width as f32 / 2.,
            self.session.document.height as f32 / 2.,
        ];
        match preset.place(&self.session.document, center) {
            Ok((objects, project)) => {
                let first = objects.first().map(|a| a.id);
                if let Some(project) = project {
                    let mut annotations = self.session.document.annotations.clone();
                    annotations.extend(objects);
                    self.execute(Command::VideoComposition {
                        project,
                        annotations,
                    });
                } else {
                    self.execute(Command::AddMany {
                        annotations: objects,
                    });
                }
                self.tool = None;
                self.shell.focus.active = false;
                self.icon_library.active = false;
                self.select_only(first);
                self.status =
                    "Block placed. Drag it to position it; each object remains editable.".into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
}
