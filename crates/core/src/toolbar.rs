//! Stable tool identities and ordering independent of a desktop widget catalog.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    Favorites,
    Arrow,
    Text,
    Callout,
    Shape,
    Stamp,
    Selection,
    Fill,
    Move,
    Blur,
    Simplify,
    MagicWand,
    Crop,
    CutOut,
    Pen,
    Line,
    Highlighter,
    Step,
    Eraser,
    Magnify,
    Redact,
}

pub const ALL: [ToolId; 21] = [
    ToolId::Favorites,
    ToolId::Arrow,
    ToolId::Text,
    ToolId::Callout,
    ToolId::Shape,
    ToolId::Stamp,
    ToolId::Selection,
    ToolId::Fill,
    ToolId::Move,
    ToolId::Blur,
    ToolId::Simplify,
    ToolId::MagicWand,
    ToolId::Crop,
    ToolId::CutOut,
    ToolId::Pen,
    ToolId::Line,
    ToolId::Highlighter,
    ToolId::Step,
    ToolId::Eraser,
    ToolId::Magnify,
    ToolId::Redact,
];
pub fn default_order() -> Vec<ToolId> {
    let mut order = vec![
        ToolId::Text,
        ToolId::Arrow,
        ToolId::Shape,
        ToolId::Callout,
        ToolId::Step,
        ToolId::Selection,
        ToolId::Move,
    ];
    for id in ALL {
        if !order.contains(&id) {
            order.push(id);
        }
    }
    order
}

/// The user-reviewed layout; explicit saved memberships always take precedence.
pub fn default_favorites() -> Vec<ToolId> {
    vec![ToolId::Move, ToolId::Text, ToolId::Shape, ToolId::Step]
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
/// Crop and Cut Out share one toolbar slot; the old Favorites identity is kept
/// solely so existing saved tool orders still deserialize without losing data.
pub fn slot(tool: ToolId) -> ToolId {
    if tool == ToolId::CutOut {
        ToolId::Crop
    } else {
        tool
    }
}
pub fn group(tool: ToolId, favorites: &[ToolId]) -> Group {
    let tool = slot(tool);
    if favorites.contains(&tool) {
        return Group::Favorites;
    }
    match tool {
        ToolId::Favorites => Group::Favorites,
        ToolId::Selection | ToolId::Move | ToolId::Blur | ToolId::Pen => Group::General,
        ToolId::Text | ToolId::Callout | ToolId::Step | ToolId::Redact | ToolId::Highlighter => {
            Group::Text
        }
        ToolId::Shape | ToolId::Arrow | ToolId::Line | ToolId::Stamp => Group::Shape,
        ToolId::Crop
        | ToolId::CutOut
        | ToolId::Fill
        | ToolId::Eraser
        | ToolId::Simplify
        | ToolId::MagicWand
        | ToolId::Magnify => Group::Canvas,
    }
}
pub fn validate_favorites(favorites: &[ToolId]) -> Result<()> {
    validate(favorites)?;
    if favorites
        .iter()
        .any(|t| matches!(t, ToolId::Favorites | ToolId::CutOut))
    {
        return Err(Error::Invalid("Invalid favorite tool slot".into()));
    }
    Ok(())
}
pub fn grouped_order(order: &[ToolId], favorites: &[ToolId]) -> Vec<ToolId> {
    let mut result = favorites.to_vec();
    for g in GROUPS.into_iter().skip(1) {
        result.extend(order.iter().copied().filter(|t| {
            !matches!(t, ToolId::Favorites | ToolId::CutOut) && group(*t, favorites) == g
        }));
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
    if tool == ToolId::Favorites
        || before.is_some_and(|b| {
            b == ToolId::Favorites || group(b, favorites) != group(tool, favorites)
        })
    {
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
    fn section_favorites_and_shared_canvas_slot_persist_and_reject_cross_section_moves() {
        let temp = tempfile::tempdir().unwrap();
        let a = Store::open(temp.path()).unwrap();
        let b = Store::open(temp.path()).unwrap();
        assert_eq!(a.read().unwrap().favorite_tools, default_favorites());
        for tool in default_favorites() {
            a.remember(Choice::FavoriteTool {
                tool,
                favorite: false,
            })
            .unwrap();
        }
        a.remember(Choice::FavoriteTool {
            tool: ToolId::Text,
            favorite: true,
        })
        .unwrap();
        b.remember(Choice::FavoriteTool {
            tool: ToolId::CutOut,
            favorite: true,
        })
        .unwrap();
        a.remember(Choice::MoveSectionTool {
            tool: ToolId::Crop,
            before: Some(ToolId::Text),
        })
        .unwrap();
        b.remember(Choice::CanvasTool(ToolId::CutOut)).unwrap();
        // Drawing defaults must not downgrade startup preferences or erase favorites.
        b.remember(Choice::DrawingStyle(
            crate::object_styles::presets(crate::Kind::Text).remove(0),
        ))
        .unwrap();
        let loaded = a.read().unwrap();
        assert_eq!(loaded.version, 8);
        assert_eq!(loaded.favorite_tools, vec![ToolId::Crop, ToolId::Text]);
        assert_eq!(loaded.canvas_tool, ToolId::CutOut);
        let order = grouped_order(&loaded.toolbar_order, &loaded.favorite_tools);
        assert_eq!(&order[..2], &[ToolId::Crop, ToolId::Text]);
        assert_eq!(order.len(), 19);
        assert!(!order.contains(&ToolId::CutOut));
        assert_eq!(order.iter().filter(|t| **t == ToolId::Crop).count(), 1);
        let path = temp.path().join("quick-controls.json");
        let saved = std::fs::read(&path).unwrap();
        assert!(
            a.remember(Choice::MoveSectionTool {
                tool: ToolId::Text,
                before: Some(ToolId::Arrow)
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        a.remember(Choice::FavoriteTool {
            tool: ToolId::Crop,
            favorite: false,
        })
        .unwrap();
        let loaded = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(group(ToolId::Crop, &loaded.favorite_tools), Group::Canvas);
        assert_eq!(loaded.canvas_tool, ToolId::CutOut);
        assert_eq!(loaded.favorite_tools, vec![ToolId::Text]);
        assert!(validate_favorites(&[ToolId::Crop, ToolId::Crop]).is_err());
        assert!(validate_favorites(&[ToolId::CutOut]).is_err());
    }
    #[test]
    fn moves_both_directions_preserve_identity_noops_and_missing_tools() {
        let mut order = vec![ToolId::Step, ToolId::Text];
        normalize(&mut order).unwrap();
        assert_eq!(&order[..2], &[ToolId::Step, ToolId::Text]);
        assert_eq!(order.len(), ALL.len());
        move_before(&mut order, ToolId::Line, Some(ToolId::Text)).unwrap();
        assert_eq!(&order[..3], &[ToolId::Step, ToolId::Line, ToolId::Text]);
        move_before(&mut order, ToolId::Step, None).unwrap();
        assert_eq!(order.last(), Some(&ToolId::Step));
        let saved = order.clone();
        move_before(&mut order, ToolId::Line, Some(ToolId::Line)).unwrap();
        assert_eq!(order, saved);
        let mut invalid = vec![ToolId::Text, ToolId::Text];
        assert!(normalize(&mut invalid).is_err());
        assert_eq!(invalid.len(), 2);
    }

    #[test]
    fn legacy_restart_and_two_writers_preserve_independent_preferences() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("quick-controls.json");
        std::fs::write(&path, br#"{"version":1,"alignment":"left","distribution":"vertical_gaps","justification":"bottom_right","canvas_snapping":false}"#).unwrap();
        let first = Store::open(temp.path()).unwrap();
        let second = Store::open(temp.path()).unwrap();
        assert_eq!(first.read().unwrap().toolbar_order, default_order());
        first
            .remember(Choice::MoveTool {
                tool: ToolId::Step,
                before: Some(ToolId::Text),
            })
            .unwrap();
        second
            .remember(Choice::Justification(Justification::CenterMiddle))
            .unwrap();
        second
            .remember(Choice::MoveTool {
                tool: ToolId::Line,
                before: Some(ToolId::Step),
            })
            .unwrap();
        let loaded = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(&loaded.toolbar_order[..2], &[ToolId::Line, ToolId::Step]);
        assert_eq!(loaded.version, 2);
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
        let path = temp.path().join("quick-controls.json");
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
