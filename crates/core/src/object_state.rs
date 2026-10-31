//! Document-local visibility/locks. Templates and pasted objects remain independent.
use crate::{Command, Document, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectState {
    #[serde(default, skip_serializing_if = "crate::text::is_false")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "crate::text::is_false")]
    pub locked: bool,
}
pub type ObjectStates = BTreeMap<u64, ObjectState>;
#[cfg(test)]
mod tests;

impl Document {
    pub fn object_state(&self, id: u64) -> ObjectState {
        self.object_states.get(&id).copied().unwrap_or_default()
    }
    pub fn object_visible(&self, id: u64) -> bool {
        !self.object_state(id).hidden
    }
    pub fn object_editable(&self, id: u64) -> bool {
        let state = self.object_state(id);
        !state.hidden && !state.locked
    }
    fn validate_object_ids(&self, ids: &[u64]) -> Result<()> {
        let unique: BTreeSet<_> = ids.iter().copied().collect();
        if ids.len() != unique.len()
            || unique
                .iter()
                .any(|id| !self.annotations.iter().any(|a| a.id == *id))
        {
            return Err(Error::Invalid("Select existing unique objects".into()));
        }
        Ok(())
    }
    pub(crate) fn set_object_states(
        &mut self,
        ids: &[u64],
        hidden: Option<bool>,
        locked: Option<bool>,
    ) -> Result<bool> {
        self.validate_object_ids(ids)?;
        let mut changed = false;
        // Group controls supply all member IDs; child controls supply one.
        for id in ids {
            let before = self.object_state(*id);
            let next = ObjectState {
                hidden: hidden.unwrap_or(before.hidden),
                locked: locked.unwrap_or(before.locked),
            };
            changed |= before != next;
            if next == ObjectState::default() {
                self.object_states.remove(id);
            } else {
                self.object_states.insert(*id, next);
            }
        }
        Ok(changed)
    }
    pub(crate) fn reorder_objects(&mut self, ids: &[u64]) -> Result<bool> {
        self.validate_object_ids(ids)?;
        if ids.len() != self.annotations.len() {
            return Err(Error::Invalid(
                "Layer order must include every object once".into(),
            ));
        }
        if ids
            .iter()
            .copied()
            .eq(self.annotations.iter().map(|a| a.id))
        {
            return Ok(false);
        }
        let mut objects: BTreeMap<_, _> = std::mem::take(&mut self.annotations)
            .into_iter()
            .map(|a| (a.id, a))
            .collect();
        self.annotations = ids.iter().map(|id| objects.remove(id).unwrap()).collect();
        Ok(true)
    }
    pub(crate) fn protect_locked_objects(&self, command: &Command) -> Result<()> {
        let ids: Vec<_> = match command {
            Command::Update { annotation } => vec![annotation.id],
            Command::UpdateMany { annotations }
            | Command::MoveAndExpand {
                annotations,
                duplicate: false,
            } => annotations.iter().map(|a| a.id).collect(),
            Command::RotateAnnotation { id, .. }
            | Command::Remove { id }
            | Command::Arrange { id, .. } => vec![*id],
            Command::ArrangeMany { ids, .. } => self.expand_groups(ids),
            Command::RemoveMany { ids }
            | Command::LayoutSelection { ids, .. }
            | Command::AlignToFirst { ids, .. } => ids.clone(),
            _ => vec![],
        };
        if ids.iter().any(|id| self.object_state(*id).locked) {
            Err(Error::Invalid(
                "Unlock the object in Objects before editing it".into(),
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn prune_object_states(&mut self) {
        let ids: BTreeSet<_> = self.annotations.iter().map(|a| a.id).collect();
        self.object_states
            .retain(|id, state| ids.contains(id) && *state != ObjectState::default());
    }
    pub(crate) fn validate_object_states(&self) -> Result<()> {
        if !self.object_states.is_empty() && self.version < 7 {
            return Err(Error::Invalid(
                "Object visibility and locks require project version 7".into(),
            ));
        }
        self.validate_object_ids(&self.object_states.keys().copied().collect::<Vec<_>>())
    }
}
