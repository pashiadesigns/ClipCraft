use super::*;
use capture_core::reusable_presets::Motion;
fn slot(a: &crate::Annotation) -> Option<usize> {
    if a.kind == crate::Kind::Step {
        Some(0)
    } else if a.image.as_ref().is_some_and(|b| b.is_icon()) {
        Some(1)
    } else {
        None
    }
}
impl Editor {
    pub(crate) fn remember_placement_motion(&mut self, a: &crate::Annotation) {
        if let Some(slot) = slot(a)
            && let Some(t) = self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.objects.get(&a.id))
        {
            self.video.placement_motion[slot] = Some(Motion::from(t));
        }
    }
    pub(crate) fn placement_command(&self, command: Command) -> Command {
        if !self.is_video() {
            return command;
        }
        let annotations = match &command {
            Command::Add { annotation } => vec![annotation.clone()],
            Command::AddMany { annotations }
            | Command::MoveAndExpand {
                annotations,
                duplicate: true,
            } => annotations.clone(),
            Command::PasteObjects { payload, center } if self.icon_library.active => {
                let Ok(annotations) = payload.place(&self.session.document, *center) else {
                    return command;
                };
                annotations
            }
            _ => return command,
        };
        // Drawn objects take their kind's motion. A Step or icon uses the one last
        // set on that kind, then its kind's motion, wherever it was placed from.
        let drawn = matches!(command, Command::Add { .. });
        let motion: std::collections::BTreeMap<_, _> = annotations
            .iter()
            .filter_map(|a| {
                let remembered = slot(a).and_then(|s| self.video.placement_motion[s].clone());
                let motion = remembered.or_else(|| {
                    (drawn || slot(a).is_some())
                        .then(|| Motion::for_new_object(a))
                        .flatten()
                })?;
                Some((a.id, motion))
            })
            .collect();
        if motion.is_empty() {
            command
        } else {
            Command::AddTimed {
                annotations,
                motion,
            }
        }
    }
    pub(crate) fn changed_placement_motion(&self, command: &Command) -> Vec<crate::Annotation> {
        let Command::Video { project } = command else {
            return vec![];
        };
        self.session
            .document
            .annotations
            .iter()
            .filter(|a| slot(a).is_some())
            .filter(|a| {
                project.objects.get(&a.id).is_some_and(|t| {
                    self.session
                        .document
                        .video
                        .as_ref()
                        .and_then(|p| p.objects.get(&a.id))
                        .is_some_and(|old| Motion::from(old) != Motion::from(t))
                })
            })
            .cloned()
            .collect()
    }
}
