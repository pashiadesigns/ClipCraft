use super::*;
use crate::{
    Command, Session,
    step_capture::{Button, Gesture, Guide},
};
use image::{Rgba, RgbaImage};

fn guide() -> Document {
    let mut g = Guide::new([320, 180]).unwrap();
    for color in [[230, 20, 40, 255], [20, 180, 220, 255]] {
        g.push(
            RgbaImage::from_pixel(320, 180, Rgba(color)),
            Gesture {
                start: [50, 40],
                end: [50, 40],
                button: Button::Left,
            },
        )
        .unwrap();
    }
    g.document().unwrap()
}
fn bytes(d: &Document) -> Vec<u8> {
    serde_json::to_vec(d).unwrap()
}

#[test]
fn blank_sections_reflow_resize_roundtrip_and_undo_without_placeholder_objects() {
    let before = guide();
    let mut s = Session::new(before.clone()).unwrap();
    execute(
        &mut s,
        Action::Insert {
            after: Some(1),
            kind: SectionKind::Blank,
        },
    );
    let blank = s.document.guide.as_ref().unwrap().sections[1].clone();
    assert_eq!(blank.kind, SectionKind::Blank);
    assert!(blank.objects.is_empty());
    assert!(blank.marker.is_none());
    assert_eq!(s.document.annotations.len(), before.annotations.len());
    assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
    let mut legacy = s.document.clone();
    legacy.version = 25;
    assert!(legacy.validate().is_err());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blank.capture.json");
    s.document.save(&path).unwrap();
    assert_eq!(bytes(&Document::load(&path).unwrap()), bytes(&s.document));
    if let Ok(dir) = std::env::var("CLIPCRAFT_PREVIEW2_FIXTURE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        s.document
            .save(&std::path::Path::new(&dir).join("blank-section.capture.json"))
            .unwrap();
    }
    for action in [
        Action::Resize {
            id: blank.id,
            height: 260.,
        },
        Action::Move {
            id: blank.id,
            index: 0,
        },
        Action::Duplicate { id: blank.id },
        Action::Remove { id: blank.id },
    ] {
        let prior = bytes(&s.document);
        execute(&mut s, action);
        s.document.validate().unwrap();
        assert_eq!(s.document.annotations.len(), before.annotations.len());
        s.execute(Command::Undo).unwrap();
        assert_eq!(bytes(&s.document), prior);
    }
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), bytes(&before));
}

#[test]
fn expanded_guide_survives_section_edits_history_reopen_and_undo() {
    let mut s = Session::new(guide()).unwrap();
    // Moving a screenshot across the edge produces a persistent raster base.
    let mut screenshot = s.document.annotations[0].clone();
    screenshot.start[0] += 120.;
    screenshot.end[0] += 120.;
    s.execute(Command::MoveAndExpand {
        annotations: vec![screenshot],
        duplicate: false,
    })
    .unwrap();
    assert!(s.document.raster.is_some());
    let dir = tempfile::tempdir().unwrap();
    let mut history = crate::history::History::open(dir.path()).unwrap();
    let mut entry = history
        .create(
            &s.document,
            "Synthetic guide",
            crate::history::Origin::Capture,
        )
        .unwrap();
    assert_eq!(bytes(&history.load(&entry.id).unwrap()), bytes(&s.document));
    let image = s.document.annotations[0].image.clone().unwrap();
    for action in [
        Action::Insert {
            after: Some(1),
            kind: SectionKind::Header,
        },
        Action::InsertImage {
            after: Some(1),
            image,
        },
        Action::Duplicate { id: 1 },
        Action::Move { id: 2, index: 0 },
        Action::Remove { id: 3 },
        Action::Remove { id: 4 },
        Action::Remove { id: 1 },
    ] {
        let before = bytes(&s.document);
        let removed = if let Action::Remove { id } = action {
            s.document
                .guide
                .as_ref()
                .unwrap()
                .sections
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .objects
                .clone()
        } else {
            vec![]
        };
        execute(&mut s, action);
        assert!(
            s.document
                .annotations
                .iter()
                .all(|a| !removed.contains(&a.id))
        );
        let after = bytes(&s.document);
        entry = history.update(&entry, &s.document).unwrap();
        let reopened = history.load(&entry.id).unwrap();
        assert_eq!(bytes(&reopened), after);
        assert_eq!(reopened.render().unwrap(), s.document.render().unwrap());
        s.execute(Command::Undo).unwrap();
        assert_eq!(bytes(&s.document), before);
        s.execute(Command::Redo).unwrap();
        assert_eq!(bytes(&s.document), after);
    }
}
fn execute(s: &mut Session, action: Action) {
    s.execute(Command::Guide { action }).unwrap();
}

#[test]
fn sections_reflow_edited_content_with_fresh_ids_and_atomic_history() {
    let mut d = guide();
    d.annotations
        .iter_mut()
        .find(|a| a.id == 2)
        .unwrap()
        .text
        .as_mut()
        .unwrap()
        .content = "User's edited caption".into();
    d.annotations[0].corner_radius = 12.;
    d.annotations[0] = d.annotations[0]
        .with_image_view(Some(
            crate::image_frame::ImageView::default().zoomed(2.).unwrap(),
        ))
        .unwrap();
    d.object_states.insert(
        1,
        crate::object_state::ObjectState {
            hidden: true,
            locked: false,
        },
    );
    let source = d.annotations[0].image.clone().unwrap();
    let mut s = Session::new(d.clone()).unwrap();
    execute(&mut s, Action::Duplicate { id: 1 });
    let layout = s.document.guide.as_ref().unwrap();
    assert_eq!(layout.sections.len(), 3);
    let duplicated = &layout.sections[1];
    let copy = s
        .document
        .annotations
        .iter()
        .find(|a| duplicated.objects.contains(&a.id) && a.kind == Kind::Image)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &source,
        copy.image.as_ref().unwrap()
    ));
    assert!(s.document.object_state(copy.id).hidden);
    assert_eq!(copy.corner_radius, 12.);
    let copy_caption = s
        .document
        .annotations
        .iter()
        .find(|a| duplicated.objects.contains(&a.id) && a.kind == Kind::Text)
        .unwrap();
    assert_eq!(
        copy_caption.text.as_ref().unwrap().content,
        "User's edited caption"
    );
    assert_eq!(
        copy.start[1] - d.annotations[0].start[1],
        layout.sections[0].height
    );
    let checkpoint = bytes(&s.document);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), bytes(&d));
    assert!(!s.can_undo());
    s.execute(Command::Redo).unwrap();
    assert_eq!(bytes(&s.document), checkpoint);
    execute(&mut s, Action::Move { id: 2, index: 0 });
    assert_eq!(s.document.guide.as_ref().unwrap().sections[0].id, 2);
    for (i, section) in s
        .document
        .guide
        .as_ref()
        .unwrap()
        .sections
        .iter()
        .enumerate()
    {
        let a = s
            .document
            .annotations
            .iter()
            .find(|a| Some(a.id) == section.marker)
            .unwrap();
        assert_eq!(a.text.as_ref().unwrap().content, (i + 1).to_string());
    }
    execute(&mut s, Action::Remove { id: 1 });
    assert!(
        !s.document
            .annotations
            .iter()
            .any(|a| [1, 2, 3].contains(&a.id))
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sections.capture.json");
    s.document.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    assert_eq!(bytes(&reopened), bytes(&s.document));
    assert_eq!(reopened.render().unwrap(), s.document.render().unwrap());
}

#[test]
fn bottom_resize_keeps_top_and_its_objects_fixed_and_reflows_following_sections() {
    let d = guide();
    let h = d.guide.as_ref().unwrap().sections[0].height;
    let grown = d
        .edit_guide(&Action::Resize {
            id: 1,
            height: h + 120.,
        })
        .unwrap();
    assert_eq!(
        grown.guide.as_ref().unwrap().top,
        d.guide.as_ref().unwrap().top
    );
    assert_eq!(grown.annotations[..4], d.annotations[..4]);
    for (a, b) in grown.annotations[4..].iter().zip(&d.annotations[4..]) {
        assert_eq!(a.start[1], b.start[1] + 120.);
    }
    assert_eq!(grown.height, d.height + 120);
    let shrunk = grown
        .edit_guide(&Action::Resize { id: 1, height: h })
        .unwrap();
    assert_eq!(bytes(&shrunk), bytes(&d));
    let minimum = d
        .edit_guide(&Action::Resize { id: 1, height: 48. })
        .unwrap();
    let end = minimum.guide.as_ref().unwrap().spans()[0].2;
    assert!(
        minimum.annotations[..4]
            .iter()
            .all(|a| a.bounds().1[1] + 12. <= end)
    );
}

#[test]
fn inserted_text_header_and_image_sections_are_editable_and_undoable() {
    let d = guide();
    for kind in [SectionKind::Header, SectionKind::Text, SectionKind::Image] {
        let mut s = Session::new(d.clone()).unwrap();
        execute(
            &mut s,
            Action::Insert {
                after: Some(1),
                kind,
            },
        );
        assert_eq!(s.document.guide.as_ref().unwrap().sections[1].kind, kind);
        s.execute(Command::Undo).unwrap();
        assert_eq!(bytes(&s.document), bytes(&d));
    }
    let mut s = Session::new(d.clone()).unwrap();
    let image = d.annotations[0].image.clone().unwrap();
    execute(
        &mut s,
        Action::InsertImage {
            after: None,
            image: image.clone(),
        },
    );
    let section = &s.document.guide.as_ref().unwrap().sections[0];
    let a = s
        .document
        .annotations
        .iter()
        .find(|a| section.objects.contains(&a.id))
        .unwrap();
    assert_eq!(a.kind, Kind::Image);
    assert!(std::sync::Arc::ptr_eq(&image, a.image.as_ref().unwrap()));
    assert!(s.document.height > d.height);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), bytes(&d));
}

#[test]
fn invalid_or_locked_changes_leave_document_and_history_intact() {
    let mut d = guide();
    d.object_states.insert(
        4,
        crate::object_state::ObjectState {
            locked: true,
            hidden: false,
        },
    );
    let mut s = Session::new(d.clone()).unwrap();
    for action in [
        Action::Resize {
            id: 1,
            height: 500.,
        },
        Action::Remove { id: 1 },
        Action::Move { id: 2, index: 0 },
        Action::Remove { id: 2 },
        Action::Move { id: 1, index: 999 },
        Action::Resize {
            id: 1,
            height: f32::NAN,
        },
        Action::Resize {
            id: 888,
            height: 100.,
        },
    ] {
        assert!(s.execute(Command::Guide { action }).is_err());
        assert_eq!(bytes(&s.document), bytes(&d));
        assert!(!s.can_undo());
    }
    let mut malformed = d.clone();
    malformed.guide.as_mut().unwrap().sections[1]
        .objects
        .push(1);
    assert!(malformed.validate().is_err());
    let mut malformed = d.clone();
    malformed.guide.as_mut().unwrap().sections[1].height = 32768.;
    assert!(malformed.validate().is_err());
    let mut malformed = d;
    malformed.version = 10;
    assert!(malformed.validate().is_err());
}

#[test]
fn ordinary_object_changes_keep_membership_and_flatten_undo_restores_sections() {
    let mut s = Session::new(guide()).unwrap();
    let d = bytes(&s.document);
    s.execute(Command::Remove { id: 1 }).unwrap();
    assert!(
        !s.document.guide.as_ref().unwrap().sections[0]
            .objects
            .contains(&1)
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), d);
    s.execute(Command::FlattenAll).unwrap();
    assert!(s.document.guide.is_none());
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), d);
    s.execute(Command::ResizeImage {
        width: 736,
        height: 1072,
    })
    .unwrap();
    s.document.validate().unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), d);
}
