//! Object-local timeline actions share the document command/Undo path.
use super::*;
use capture_core::video::editor::Animation;

#[derive(Clone, Copy, Debug)]
pub(super) enum Action {
    Rename,
    CopyFormat,
    PasteFormat,
    Duplicate,
    Delete,
    Visible(bool),
    Locked(bool),
    MoveStart,
    TrimStart,
    TrimEnd,
    Track(Option<u64>),
    ClearMotion,
    Seek(bool),
}

pub(super) fn show(
    ui: &mut egui::Ui,
    v: &Project,
    id: u64,
    state: capture_core::object_state::ObjectState,
    name: &str,
) -> Option<Action> {
    let t = v.objects.get(&id)?;
    let mut action = None;
    ui.set_min_width(230.);
    ui.label(egui::RichText::new(name).strong());
    ui.separator();
    let mut item = |ui: &mut egui::Ui, label: &str, enabled: bool, value| {
        let r = ui
            .add_enabled_ui(enabled, |ui| crate::controls::menu_action(ui, label))
            .inner;
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(("timeline-menu", label)), r.rect));
        if r.clicked() {
            action = Some(value);
            ui.close();
        }
    };
    item(ui, "Copy formatting", true, Action::CopyFormat);
    item(
        ui,
        "Paste shared formatting",
        !state.locked,
        Action::PasteFormat,
    );
    item(ui, "Rename…", !state.locked, Action::Rename);
    item(ui, "Duplicate", !state.locked, Action::Duplicate);
    ui.separator();
    let inside = v.cursor > t.start && v.cursor < t.end;
    item(
        ui,
        "Move start to playhead",
        !state.locked && v.cursor != t.start && v.cursor + (t.end - t.start) <= 14_400_000,
        Action::MoveStart,
    );
    item(
        ui,
        "Trim start to playhead",
        !state.locked && inside,
        Action::TrimStart,
    );
    item(
        ui,
        "Trim end to playhead",
        !state.locked && inside,
        Action::TrimEnd,
    );
    ui.add_enabled_ui(!state.locked, |ui| {
        crate::controls::menu_submenu(ui, "Move to track", |ui| {
            for (i, track) in v.tracks.iter().enumerate() {
                item(
                    ui,
                    &format!("Track {}", i + 1),
                    *track != t.track && v.free(*track, id, t.start, t.end),
                    Action::Track(Some(*track)),
                );
            }
            ui.separator();
            item(ui, "New track", v.tracks.len() < 1000, Action::Track(None));
        });
    });
    item(
        ui,
        "Clear animations",
        !state.locked && (t.entrance != Animation::None || t.exit != Animation::None),
        Action::ClearMotion,
    );
    ui.separator();
    item(ui, "Jump to object start", true, Action::Seek(false));
    item(ui, "Jump to object end", true, Action::Seek(true));
    ui.separator();
    item(
        ui,
        if state.hidden {
            "Show object"
        } else {
            "Hide object"
        },
        true,
        Action::Visible(state.hidden),
    );
    item(
        ui,
        if state.locked {
            "Unlock object"
        } else {
            "Lock object"
        },
        true,
        Action::Locked(!state.locked),
    );
    ui.separator();
    item(ui, "Delete object", !state.locked, Action::Delete);
    action
}

impl Editor {
    pub(super) fn video_object_action(&mut self, id: u64, action: Action) {
        let Some(mut v) = self.session.document.video.clone() else {
            return;
        };
        let Some(mut t) = v.objects.get(&id).cloned() else {
            return;
        };
        let locked = self.session.document.object_state(id).locked;
        if locked
            && !matches!(
                action,
                Action::Seek(_) | Action::Visible(_) | Action::Locked(_)
            )
        {
            self.status = "Unlock the object before editing it.".into();
            return;
        }
        self.video.pause();
        self.select_only(Some(id));
        self.video.selected = Selection::Objects;
        match action {
            Action::CopyFormat => {
                self.quick_controls.copied_format = self
                    .session
                    .document
                    .annotations
                    .iter()
                    .find(|a| a.id == id)
                    .cloned();
                return;
            }
            Action::PasteFormat => {
                self.paste_shared_format();
                return;
            }
            Action::Rename => {
                let name = if t.label.is_empty() {
                    self.session
                        .document
                        .annotations
                        .iter()
                        .find(|a| a.id == id)
                        .map(|a| {
                            a.text
                                .as_ref()
                                .map(|x| x.content.clone())
                                .unwrap_or_else(|| {
                                    if a.focus.is_some() {
                                        "Focus".into()
                                    } else {
                                        format!("{:?}", a.kind)
                                    }
                                })
                        })
                        .unwrap_or_default()
                } else {
                    t.label
                };
                self.video.rename = Some((id, name, true));
                return;
            }
            Action::Duplicate => {
                let before = self.session.document.annotations.len();
                self.execute(Command::VideoDuplicate { id, timing: None });
                if self.session.document.annotations.len() > before {
                    let new_id = self.session.document.annotations.last().unwrap().id;
                    self.select_only(Some(new_id));
                }
                return;
            }
            Action::Delete => {
                self.execute(Command::Remove { id });
                return;
            }
            Action::Visible(visible) => {
                self.execute(Command::SetObjectsVisible {
                    ids: vec![id],
                    visible,
                });
                return;
            }
            Action::Locked(locked) => {
                self.execute(Command::SetObjectsLocked {
                    ids: vec![id],
                    locked,
                });
                return;
            }
            Action::Seek(end) => {
                self.video_seek(if end {
                    t.end.saturating_sub(1)
                } else {
                    t.start
                });
                return;
            }
            Action::MoveStart => {
                let length = t.end - t.start;
                if v.cursor + length > v.duration() {
                    self.status = "Move the playhead earlier so the entire object fits.".into();
                    return;
                }
                t.start = v.cursor;
                t.end = t.start + length;
            }
            Action::TrimStart | Action::TrimEnd => {
                if v.cursor <= t.start || v.cursor >= t.end {
                    self.status = "Move the playhead inside this object before trimming.".into();
                    return;
                }
                if matches!(action, Action::TrimStart) {
                    t.start = v.cursor;
                } else {
                    t.end = v.cursor;
                }
            }
            Action::Track(track) => {
                t.track = match track {
                    Some(track)
                        if v.tracks.contains(&track) && v.free(track, id, t.start, t.end) =>
                    {
                        track
                    }
                    None if v.tracks.len() < 1000 => v.add_track(),
                    _ => {
                        self.status =
                            "Choose a track with no overlapping object at this time.".into();
                        return;
                    }
                };
            }
            Action::ClearMotion => {
                t.entrance = Animation::None;
                t.exit = Animation::None;
                t.entrance_stack.clear();
                t.exit_stack.clear();
            }
        }
        t.clamp_animations();
        v.place(id, t);
        if !v.same_edit(self.session.document.video.as_ref().unwrap()) {
            self.commit_video(v);
        }
    }
}
