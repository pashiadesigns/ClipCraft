//! Stable tool identities and ordering independent of a desktop widget catalog.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    Move,
    // Persisted identities are independent of visible tool names.
    #[serde(rename = "arrow")]
    Lines,
    Text,
    Shape,
    Focus,
    Icons,
    Steps,
    #[serde(rename = "pen")]
    Penlighter,
    Obfuscate,
    Crop,
    CutOut,
    Eraser,
}

pub const ALL: [ToolId; 12] = [
    ToolId::Move,
    ToolId::Lines,
    ToolId::Text,
    ToolId::Shape,
    ToolId::Icons,
    ToolId::Steps,
    ToolId::Penlighter,
    ToolId::Obfuscate,
    ToolId::Crop,
    ToolId::CutOut,
    ToolId::Eraser,
    ToolId::Focus,
];
pub fn default_order() -> Vec<ToolId> {
    let mut order = vec![ToolId::Move, ToolId::Text, ToolId::Shape, ToolId::Steps];
    for id in ALL {
        if !order.contains(&id) {
            order.push(id);
        }
    }
    order
}

/// The user-reviewed layout; explicit saved memberships always take precedence.
pub fn default_favorites() -> Vec<ToolId> {
    vec![ToolId::Move, ToolId::Text, ToolId::Shape, ToolId::Steps]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Favorites,
    General,
    Text,
    Shape,
    Canvas,
}
pub const GROUPS: [Group; 5] = [
    Group::Favorites,
    Group::General,
    Group::Text,
    Group::Shape,
    Group::Canvas,
];
impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Self::Favorites => "Favorites",
            Self::General => "General Tools",
            Self::Text => "Text Tools",
            Self::Shape => "Shape Tools",
            Self::Canvas => "Canvas Tools",
        }
    }
}
/// Each supported tool has a distinct toolbar slot.
pub fn slot(tool: ToolId) -> ToolId {
    tool
}
pub fn group(tool: ToolId, favorites: &[ToolId]) -> Group {
    let tool = slot(tool);
    if favorites.contains(&tool) {
        return Group::Favorites;
    }
    match tool {
        ToolId::Move | ToolId::Penlighter | ToolId::Obfuscate => Group::General,
        ToolId::Text | ToolId::Steps => Group::Text,
        ToolId::Focus | ToolId::Shape | ToolId::Lines | ToolId::Icons => Group::Shape,
        ToolId::Crop | ToolId::CutOut | ToolId::Eraser => Group::Canvas,
    }
}
pub fn validate_favorites(favorites: &[ToolId]) -> Result<()> {
    validate(favorites)?;
    Ok(())
}
pub fn grouped_order(order: &[ToolId], favorites: &[ToolId]) -> Vec<ToolId> {
    let mut result = favorites.to_vec();
    for g in GROUPS.into_iter().skip(1) {
        result.extend(order.iter().copied().filter(|t| group(*t, favorites) == g));
    }
    result
}
pub fn move_in_group(
    order: &mut Vec<ToolId>,
    favorites: &mut Vec<ToolId>,
    tool: ToolId,
    before: Option<ToolId>,
) -> Result<()> {
    let tool = slot(tool);
    let before = before.map(slot);
    if before.is_some_and(|b| group(b, favorites) != group(tool, favorites)) {
        return Err(Error::Invalid(
            "Drag tools within their section; use Add to Favorites to change sections".into(),
        ));
    }
    if before == Some(tool) {
        return Ok(());
    }
    if group(tool, favorites) == Group::Favorites {
        favorites.retain(|t| *t != tool);
        let i = before
            .and_then(|b| favorites.iter().position(|t| *t == b))
            .unwrap_or(favorites.len());
        favorites.insert(i, tool);
        Ok(())
    } else {
        move_before(order, tool, before)
    }
}

pub fn validate(order: &[ToolId]) -> Result<()> {
    if order.len() > ALL.len()
        || order
            .iter()
            .enumerate()
            .any(|(i, id)| order[..i].contains(id))
    {
        return Err(Error::Invalid(
            "Toolbar order contains duplicate tools; preferences preserved".into(),
        ));
    }
    Ok(())
}

/// Retain saved priorities and append newly introduced or previously absent tools.
pub fn normalize(order: &mut Vec<ToolId>) -> Result<()> {
    validate(order)?;
    for tool in ALL {
        if !order.contains(&tool) {
            order.push(tool);
        }
    }
    Ok(())
}

/// Move by identity, so a stale window does not overwrite another preference or
/// depend on a catalog's numeric positions. `None` means the end of the list.
pub fn move_before(order: &mut Vec<ToolId>, tool: ToolId, before: Option<ToolId>) -> Result<()> {
    normalize(order)?;
    if before == Some(tool) {
        return Ok(());
    }
    order.retain(|id| *id != tool);
    let index = before
        .and_then(|id| order.iter().position(|t| *t == id))
        .unwrap_or(order.len());
    order.insert(index, tool);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_preferences::{Choice, Justification, Store};
    #[test]
    fn independent_crop_and_cut_out_favorites_preserve_order() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        for tool in default_favorites() {
            store
                .remember(Choice::FavoriteTool {
                    tool,
                    favorite: false,
                })
                .unwrap();
        }
        for tool in [ToolId::Text, ToolId::Crop, ToolId::CutOut] {
            store
                .remember(Choice::FavoriteTool {
                    tool,
                    favorite: true,
                })
                .unwrap();
        }
        store
            .remember(Choice::MoveSectionTool {
                tool: ToolId::CutOut,
                before: Some(ToolId::Text),
            })
            .unwrap();
        let saved = store.read().unwrap();
        assert_eq!(
            saved.favorite_tools,
            [ToolId::CutOut, ToolId::Text, ToolId::Crop]
        );
        let order = grouped_order(&saved.toolbar_order, &saved.favorite_tools);
        assert_eq!(order.len(), ALL.len());
        assert!(
            store
                .remember(Choice::MoveSectionTool {
                    tool: ToolId::Lines,
                    before: Some(ToolId::Text)
                })
                .is_err()
        );
        assert_eq!(store.read().unwrap(), saved);
    }
    #[test]
    fn moves_both_directions_preserve_identity_noops_and_missing_tools() {
        let mut order = vec![ToolId::Steps, ToolId::Text];
        normalize(&mut order).unwrap();
        assert_eq!(&order[..2], &[ToolId::Steps, ToolId::Text]);
        assert_eq!(order.len(), ALL.len());
        move_before(&mut order, ToolId::Icons, Some(ToolId::Text)).unwrap();
        assert_eq!(&order[..3], &[ToolId::Steps, ToolId::Icons, ToolId::Text]);
        move_before(&mut order, ToolId::Steps, None).unwrap();
        assert_eq!(order.last(), Some(&ToolId::Steps));
        let saved = order.clone();
        move_before(&mut order, ToolId::Icons, Some(ToolId::Icons)).unwrap();
        assert_eq!(order, saved);
        let mut invalid = vec![ToolId::Text, ToolId::Text];
        assert!(normalize(&mut invalid).is_err());
        assert_eq!(invalid.len(), 2);
    }

    #[test]
    fn restart_and_two_writers_preserve_independent_preferences() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("quick-controls-v2.json");
        std::fs::write(&path, br#"{"version":12,"alignment":"left","distribution":"vertical_gaps","justification":"bottom_right","canvas_snapping":false}"#).unwrap();
        let first = Store::open(temp.path()).unwrap();
        let second = Store::open(temp.path()).unwrap();
        assert_eq!(first.read().unwrap().toolbar_order, default_order());
        first
            .remember(Choice::MoveTool {
                tool: ToolId::Steps,
                before: Some(ToolId::Text),
            })
            .unwrap();
        second
            .remember(Choice::Justification(Justification::CenterMiddle))
            .unwrap();
        second
            .remember(Choice::MoveTool {
                tool: ToolId::Icons,
                before: Some(ToolId::Steps),
            })
            .unwrap();
        let loaded = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(&loaded.toolbar_order[1..3], &[ToolId::Icons, ToolId::Steps]);
        assert_eq!(loaded.version, 12);
        assert!(!loaded.canvas_snapping);
        assert_eq!(loaded.justification, Justification::CenterMiddle);
        let reset = first.remember(Choice::ResetToolbar).unwrap();
        assert_eq!(reset.toolbar_order, default_order());
        assert_eq!(reset.justification, Justification::CenterMiddle);
        assert!(!reset.canvas_snapping);
    }

    #[test]
    fn duplicate_unknown_and_locked_preferences_cannot_overwrite_saved_state() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("quick-controls-v2.json");
        let store = Store::open(temp.path()).unwrap();
        store
            .remember(Choice::MoveTool {
                tool: ToolId::Text,
                before: Some(ToolId::Text),
            })
            .unwrap();
        let saved = std::fs::read(&path).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp.path().join("quick-controls.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        assert!(store.remember(Choice::ResetToolbar).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        drop(lock);
        for order in [
            serde_json::json!(["text", "text"]),
            serde_json::json!(["unknown_future_tool"]),
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
            value["toolbar_order"] = order;
            let bytes = serde_json::to_vec(&value).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            assert!(Store::open(temp.path()).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }
}
