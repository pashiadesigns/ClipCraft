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
        let motion: std::collections::BTreeMap<_, _> = annotations
            .iter()
            .filter_map(|a| Some((a.id, self.video.placement_motion[slot(a)?].clone()?)))
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
