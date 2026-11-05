use super::*;
use crate::{Kind, Session};

#[test]
fn child_visibility_and_lock_are_independent_with_persistent_group_membership() {
    let mut s = Session::new(fixture()).unwrap();
    let ids: Vec<_> = s
        .document
        .annotations
        .iter()
        .take(2)
        .map(|a| a.id)
        .collect();
    s.execute(Command::GroupObjects { ids: ids.clone() })
        .unwrap();
    let grouped = s.document.clone();
    s.execute(Command::SetObjectsLocked {
        ids: vec![ids[0]],
        locked: true,
    })
    .unwrap();
    assert!(s.document.object_state(ids[0]).locked);
    assert!(!s.document.object_state(ids[1]).locked);
    let mut sibling = s
        .document
        .annotations
        .iter()
        .find(|a| a.id == ids[1])
        .unwrap()
        .clone();
    sibling.color = [80, 100, 220, 255];
    s.execute(Command::Update {
        annotation: sibling,
    })
    .unwrap();
    assert!(s.execute(Command::Remove { id: ids[0] }).is_err());
    s.execute(Command::SetObjectsVisible {
        ids: vec![ids[0]],
        visible: false,
    })
    .unwrap();
    assert!(!s.document.object_visible(ids[0]));
    assert!(s.document.object_visible(ids[1]));
    assert_eq!(s.document.group_members(ids[0]), ids);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("child.capture.json");
    s.document.save(&path).unwrap();
    assert_eq!(
        Document::load(&path).unwrap().object_states,
        s.document.object_states
    );
    for _ in 0..3 {
        s.execute(Command::Undo).unwrap();
    }
    assert_eq!(
        serde_json::to_vec(&s.document).unwrap(),
        serde_json::to_vec(&grouped).unwrap()
    );
}

fn fixture() -> Document {
    let mut d: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let template = d
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Rectangle)
        .unwrap()
        .clone();
    for (id, kind) in [(90, Kind::Blur), (91, Kind::Pixelate)] {
        let mut a = template.clone();
        a.id = id;
        a.kind = kind;
        a.text = None;
        a.fill = None;
        a.corner_radius = 0.;
        a.width = 6;
        d.annotations.push(a);
    }
    d.validate().unwrap();
    d
}

#[test]
fn visibility_order_lock_state_roundtrip_render_and_undo_are_atomic() {
    let original = fixture();
    let source = original.source_png.clone();
    let before = original.render().unwrap();
    let mut s = Session::new(original.clone()).unwrap();
    let ids: Vec<_> = s.document.annotations.iter().map(|a| a.id).collect();
    let hide: Vec<_> = ids.iter().step_by(2).copied().collect();
    s.execute(Command::SetObjectsVisible {
        ids: hide.clone(),
        visible: false,
    })
    .unwrap();
    assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
    let mut expected = original.clone();
    expected.annotations.retain(|a| !hide.contains(&a.id));
    let rendered = s.document.render().unwrap();
    assert_eq!(rendered, expected.render().unwrap());
    assert_eq!(
        s.document
            .display_list()
            .unwrap()
            .render_viewport(crate::display::Viewport {
                size: [640, 400],
                origin: [0., 0.],
                scale: 1.
            })
            .unwrap(),
        rendered
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("layers.capture.json");
    s.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded.object_states, s.document.object_states);
    assert_eq!(loaded.render().unwrap(), rendered);
    s.execute(Command::SetObjectsVisible {
        ids: hide,
        visible: false,
    })
    .unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.render().unwrap(), before);
    assert!(!s.can_undo());
    s.execute(Command::Redo).unwrap();
    s.execute(Command::SetObjectsVisible {
        ids: ids.clone(),
        visible: false,
    })
    .unwrap();
    assert_eq!(
        s.document.render().unwrap(),
        s.document.base_image().unwrap()
    );
    assert!(s.document.display_list().unwrap().layers.is_empty());
    s.execute(Command::SetObjectsVisible {
        ids: ids.clone(),
        visible: true,
    })
    .unwrap();
    assert!(s.document.object_states.is_empty());
    assert_eq!(s.document.render().unwrap(), before);
    let original_order = s.document.annotations.clone();
    let mut reverse = ids.clone();
    reverse.reverse();
    s.execute(Command::ReorderObjects {
        ids: reverse.clone(),
    })
    .unwrap();
    assert!(s.document.annotations.iter().map(|a| a.id).eq(reverse));
    expected = original.clone();
    expected.annotations.reverse();
    assert_eq!(s.document.render().unwrap(), expected.render().unwrap());
    s.execute(Command::ReorderObjects {
        ids: s.document.annotations.iter().map(|a| a.id).collect(),
    })
    .unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations, original_order);
    s.execute(Command::SetObjectsLocked {
        ids: vec![ids[0]],
        locked: true,
    })
    .unwrap();
    assert_eq!(s.document.render().unwrap(), before);
    assert!(!s.document.object_editable(ids[0]));
    s.save(&path).unwrap();
    assert!(Document::load(&path).unwrap().object_state(ids[0]).locked);
    assert_eq!(s.document.source_png, source);
    let payload = crate::clipboard_payload::ObjectPayload::capture(&s.document, &ids[..1]).unwrap();
    payload.validate().unwrap();
    assert_eq!(
        payload.document_version,
        crate::MAX_PROJECT_VERSION,
        "document-local lock state is not pasted into another project"
    );
    s.execute(Command::SetObjectsLocked {
        ids: vec![ids[0]],
        locked: false,
    })
    .unwrap();
    assert!(s.document.object_editable(ids[0]));
}

#[test]
fn locks_reject_accidental_edits_invalid_batches_preserve_state_and_removed_metadata_cleans_up() {
    let d = fixture();
    let mut s = Session::new(d).unwrap();
    let id = s.document.annotations[0].id;
    s.execute(Command::SetObjectsLocked {
        ids: vec![id],
        locked: true,
    })
    .unwrap();
    let before = serde_json::to_vec(&s.document).unwrap();
    let a = s.document.annotations[0].clone();
    for command in [
        Command::Remove { id },
        Command::RemoveMany { ids: vec![id] },
        Command::Update {
            annotation: a.clone(),
        },
        Command::UpdateMany {
            annotations: vec![a],
        },
        Command::RotateAnnotation { id, degrees: 20. },
        Command::SetObjectsVisible {
            ids: vec![id, 99999],
            visible: false,
        },
        Command::SetObjectsLocked {
            ids: vec![id, id],
            locked: false,
        },
        Command::ReorderObjects { ids: vec![id] },
    ] {
        assert!(s.execute(command).is_err());
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), before);
    }
    s.execute(Command::Undo).unwrap();
    assert!(!s.can_undo());
    assert!(s.document.object_states.is_empty());
    s.execute(Command::SetObjectsVisible {
        ids: vec![id],
        visible: false,
    })
    .unwrap();
    s.execute(Command::Remove { id }).unwrap();
    assert!(s.document.object_states.is_empty());
    s.execute(Command::Undo).unwrap();
    assert!(s.document.object_state(id).hidden);
    let mut invalid = s.document.clone();
    invalid.version = 6;
    assert!(invalid.validate().is_err());
    invalid = s.document.clone();
    invalid.object_states.insert(
        99999,
        ObjectState {
            hidden: true,
            locked: false,
        },
    );
    assert!(invalid.validate().is_err());
    let state = s.document.object_states.clone();
    s.execute(Command::FlattenAll).unwrap();
    assert!(s.document.object_states.is_empty());
    s.document.validate().unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.object_states, state);
    let mut legacy = fixture();
    legacy.object_states.clear();
    let bytes = serde_json::to_string(&legacy).unwrap();
    assert!(!bytes.contains("object_states"));
}
