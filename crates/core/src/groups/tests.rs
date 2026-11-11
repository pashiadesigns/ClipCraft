use super::*;
use crate::{Command, Kind, Session};
fn document() -> Document {
    serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap()
}
fn bytes(d: &Document) -> Vec<u8> {
    serde_json::to_vec(d).unwrap()
}
#[test]
fn membership_copy_duplicate_lock_prune_reopen_and_undo_are_atomic() {
    let d = document();
    let original = bytes(&d);
    let output = d.render().unwrap();
    let ids: Vec<_> = d.annotations.iter().take(2).map(|a| a.id).collect();
    let mut s = Session::new(d).unwrap();
    s.execute(Command::GroupObjects { ids: ids.clone() })
        .unwrap();
    assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
    assert_eq!(s.document.group_members(ids[0]), ids);
    assert_eq!(s.document.render().unwrap(), output);
    let grouped = bytes(&s.document);
    s.execute(Command::GroupObjects { ids: ids.clone() })
        .unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), original);
    assert!(!s.can_undo());
    s.execute(Command::Redo).unwrap();
    let copies = s.document.duplicate_selection(&ids).unwrap();
    assert!(copies.iter().all(|a| a.group == copies[0].group));
    assert_ne!(copies[0].group, s.document.annotations[0].group);
    let payload = crate::clipboard_payload::ObjectPayload::capture(&s.document, &ids[..1]).unwrap();
    assert_eq!(payload.objects.len(), 2);
    s.execute(Command::PasteObjects {
        payload,
        center: [480., 270.],
    })
    .unwrap();
    let pasted = &s.document.annotations[s.document.annotations.len() - 2..];
    assert!(pasted.iter().all(|a| a.group == pasted[0].group));
    assert_ne!(pasted[0].group, s.document.annotations[0].group);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), grouped);
    s.execute(Command::SetObjectsLocked {
        ids: ids.clone(),
        locked: true,
    })
    .unwrap();
    assert!(ids.iter().all(|id| s.document.object_state(*id).locked));
    let locked = bytes(&s.document);
    assert!(
        s.execute(Command::UngroupObjects { ids: ids.clone() })
            .is_err()
    );
    assert_eq!(bytes(&s.document), locked);
    s.execute(Command::Undo).unwrap();
    s.execute(Command::UngroupObjects {
        ids: ids[..1].to_vec(),
    })
    .unwrap();
    assert!(s.document.annotations.iter().all(|a| a.group.is_none()));
    s.execute(Command::Undo).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("group.capture.json");
    s.save(&path).unwrap();
    assert_eq!(bytes(&Document::load(&path).unwrap()), grouped);
    s.execute(Command::Remove { id: ids[0] }).unwrap();
    assert!(s.document.annotations.iter().all(|a| a.group.is_none()));
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), grouped);
    let mut invalid = s.document.clone();
    invalid.version = 15;
    assert!(invalid.validate().is_err());
    invalid.version = 16;
    invalid.annotations[0].group = Some(99);
    assert!(invalid.validate().is_err());
}
#[test]
fn shared_center_transform_keeps_contents_spacing_and_source_with_one_undo() {
    let mut d = document();
    d.annotations.retain(|a| {
        matches!(
            a.kind,
            Kind::Rectangle | Kind::Text | Kind::Callout | Kind::Step
        )
    });
    let ids = d.annotations.iter().map(|a| a.id).collect();
    let mut s = Session::new(d).unwrap();
    s.execute(Command::GroupObjects { ids }).unwrap();
    let original = bytes(&s.document);
    let objects = s.document.annotations.clone();
    let (min, max) = bounds(&objects).unwrap();
    let center = [0, 1].map(|i| (min[i] + max[i]) * 0.5);
    let scaled = scaled(&objects, center, 0.75).unwrap();
    let rotated = rotated(&scaled, center, 37.).unwrap();
    for (old, new) in objects.iter().zip(&rotated) {
        assert_eq!(old.group, new.group);
        assert_eq!(
            old.text.as_ref().map(|t| &t.content),
            new.text.as_ref().map(|t| &t.content)
        );
        assert!((new.rotation_degrees - old.rotation_degrees - 37.).abs() < 0.01);
    }
    s.execute(Command::MoveAndExpand {
        annotations: rotated,
        duplicate: false,
    })
    .unwrap();
    let after = bytes(&s.document);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), original);
    s.execute(Command::Redo).unwrap();
    assert_eq!(bytes(&s.document), after);
    assert!(super::scaled(&objects, center, f32::NAN).is_err());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transformed.capture.json");
    s.save(&path).unwrap();
    assert_eq!(
        Document::load(&path).unwrap().render().unwrap(),
        s.document.render().unwrap()
    );
    if let Ok(root) = std::env::var("CLIPDESIGNER_GROUP_DIR") {
        let root = std::path::Path::new(&root);
        std::fs::create_dir_all(root).unwrap();
        s.document.save(&root.join("groups.capture.json")).unwrap();
        s.document.export_png(&root.join("groups.png")).unwrap();
    }
}
#[test]
fn section_duplication_remaps_group_identity_and_removal_prunes_membership() {
    use crate::guide_sections::{Action, Layout, Section, SectionKind};
    let mut d = Document::blank(200, 250).unwrap();
    for (id, x) in [(1, 40.), (2, 90.)] {
        d.annotations.push(
            crate::steps::marker(
                id,
                [x, 80.],
                24,
                [240, 80, 50, 255],
                (id as u32, false),
                [200, 250],
            )
            .unwrap(),
        );
    }
    d.guide = Some(Layout {
        pages: None,
        template: None,
        top: 20.,
        sections: vec![Section {
            page: 0,
            id: 1,
            kind: SectionKind::Image,
            height: 200.,
            objects: vec![1, 2],
            marker: Some(1),
        }],
    });
    d.promote_features();
    let mut s = Session::new(d).unwrap();
    s.execute(Command::GroupObjects { ids: vec![1, 2] })
        .unwrap();
    let before = bytes(&s.document);
    s.execute(Command::Guide {
        action: Action::Duplicate { id: 1 },
    })
    .unwrap();
    assert_eq!(s.document.group_members(1), vec![1, 2]);
    let copies = &s.document.annotations[2..];
    assert_eq!(copies[0].group, copies[1].group);
    assert_ne!(copies[0].group, s.document.annotations[0].group);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), before);
}

#[test]
fn arranging_a_group_preserves_its_members_order_and_respects_neighbor_groups() {
    let mut s = Session::new(document()).unwrap();
    let ids: Vec<_> = s.document.annotations.iter().map(|a| a.id).collect();
    assert!(ids.len() >= 4);
    s.execute(Command::GroupObjects {
        ids: ids[..2].to_vec(),
    })
    .unwrap();
    s.execute(Command::GroupObjects {
        ids: ids[2..4].to_vec(),
    })
    .unwrap();
    let original = bytes(&s.document);
    s.execute(Command::ArrangeMany {
        ids: ids[..1].to_vec(),
        order: crate::StackOrder::BringForward,
    })
    .unwrap();
    assert_eq!(
        s.document
            .annotations
            .iter()
            .take(4)
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[3], ids[0], ids[1]]
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), original);
    s.execute(Command::ArrangeMany {
        ids: ids[..1].to_vec(),
        order: crate::StackOrder::BringToFront,
    })
    .unwrap();
    assert_eq!(
        s.document
            .annotations
            .iter()
            .rev()
            .take(2)
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![ids[1], ids[0]]
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), original);
    s.execute(Command::ArrangeMany {
        ids: ids[..1].to_vec(),
        order: crate::StackOrder::SendBackward,
    })
    .unwrap();
    assert!(s.can_redo());
    let objects = s.document.annotations.clone();
    let mut invalid = objects.clone();
    invalid[0].kind = Kind::Blur;
    assert!(rotated(&invalid, [100., 100.], 20.).is_err());
}
