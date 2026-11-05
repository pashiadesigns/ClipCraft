use super::*;
use crate::{Command, Session};

fn template() -> Document {
    crate::templates::build(
        "two-steps",
        "Synthetic page guide",
        &[],
        crate::templates::Scale::Fill,
    )
    .unwrap()
}
fn bytes(d: &Document) -> Vec<u8> {
    serde_json::to_vec(d).unwrap()
}
fn execute(s: &mut Session, action: Action) {
    s.execute(Command::Guide { action }).unwrap();
}

#[test]
fn section_relocation_across_pages_preserves_sources_and_is_one_atomic_undo() {
    let original = template()
        .edit_guide(&Action::AddPage {
            page: 0,
            copy: false,
        })
        .unwrap();
    let id = original.guide.as_ref().unwrap().sections[1].id;
    let members = original.guide.as_ref().unwrap().sections[1].objects.clone();
    let mut session = Session::new(original.clone()).unwrap();
    execute(
        &mut session,
        Action::Relocate {
            id,
            page: 1,
            index: 0,
        },
    );
    let after = &session.document;
    assert_eq!(
        after
            .guide
            .as_ref()
            .unwrap()
            .sections
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .page,
        1
    );
    for object in members {
        let before = original
            .annotations
            .iter()
            .find(|a| a.id == object)
            .unwrap();
        let moved = after.annotations.iter().find(|a| a.id == object).unwrap();
        assert!(moved.start[0] > before.start[0]);
        assert_eq!(before.image, moved.image);
    }
    let json = bytes(after);
    assert_eq!(
        json,
        bytes(&serde_json::from_slice::<Document>(&json).unwrap())
    );
    session.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&session.document), bytes(&original));
    assert!(!session.can_undo());
    let guide = original.guide.as_ref().unwrap();
    let last = guide.sections.iter().filter(|s| s.page == 0).count() - 1;
    execute(
        &mut session,
        Action::Relocate {
            id: guide.sections[0].id,
            page: 0,
            index: last,
        },
    );
    assert_eq!(
        session.document.guide.as_ref().unwrap().sections[last].id,
        guide.sections[0].id
    );
    session.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&session.document), bytes(&original));
    assert!(
        original
            .edit_guide(&Action::Relocate {
                id,
                page: 9,
                index: 0
            })
            .is_err()
    );
    assert!(
        original
            .edit_guide(&Action::Relocate {
                id,
                page: 1,
                index: 99
            })
            .is_err()
    );
}

#[test]
fn page_copy_delete_move_roundtrip_and_atomic_undo_preserve_objects() {
    let original = template();
    let mut s = Session::new(original.clone()).unwrap();
    execute(
        &mut s,
        Action::AddPage {
            page: 0,
            copy: true,
        },
    );
    let g = s.document.guide.as_ref().unwrap();
    assert_eq!(g.page_count(), 2);
    assert_eq!(s.document.annotations.len(), original.annotations.len() * 2);
    assert_eq!(s.document.width, original.width * 2 + PAGE_GAP);
    assert_eq!(s.document.height, original.height);
    assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
    let ids: BTreeSet<_> = s.document.annotations.iter().map(|a| a.id).collect();
    assert_eq!(ids.len(), s.document.annotations.len());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pages.capture.json");
    s.document.save(&path).unwrap();
    assert_eq!(bytes(&s.document), bytes(&Document::load(&path).unwrap()));
    let copied = bytes(&s.document);
    execute(&mut s, Action::DeletePage { page: 0 });
    assert_eq!(s.document.annotations.len(), original.annotations.len());
    assert_eq!(s.document.guide.as_ref().unwrap().page_count(), 1);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), copied);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), bytes(&original));

    execute(
        &mut s,
        Action::AddPage {
            page: 0,
            copy: false,
        },
    );
    let before_move = bytes(&s.document);
    let id = s.document.guide.as_ref().unwrap().sections[1].id;
    execute(
        &mut s,
        Action::SectionPage {
            id,
            page: 1,
            copy: false,
        },
    );
    assert_eq!(
        s.document
            .guide
            .as_ref()
            .unwrap()
            .sections
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .page,
        1
    );
    assert_eq!(s.document.annotations.len(), original.annotations.len());
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), before_move);
    execute(
        &mut s,
        Action::SectionPage {
            id,
            page: 1,
            copy: true,
        },
    );
    assert!(s.document.annotations.len() > original.annotations.len());
    s.document.validate().unwrap();
}

#[test]
fn overflow_respects_whole_sections_dimensions_and_pdf_pages() {
    let mut s = Session::new(template()).unwrap();
    execute(
        &mut s,
        Action::PageSize {
            width: 900,
            height: 550,
            auto_overflow: true,
        },
    );
    let g = s.document.guide.as_ref().unwrap();
    assert_eq!(g.page_count(), 2);
    for (index, (_, _, bottom)) in g.spans().iter().enumerate() {
        assert!(*bottom <= g.page_origin(g.sections[index].page) + 550.);
    }
    let pages = crate::export::PageSettings::default()
        .document_pages(&s.document)
        .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1].source, [900 + PAGE_GAP, 0, 900, 550]);
    let pdf = crate::export::encode_document(
        &s.document,
        &crate::export::Options {
            format: crate::export::Format::Pdf,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 2"));
    if let Ok(dir) = std::env::var("CLIPCRAFT_FRAME_PAGES_FIXTURES") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("guide-pages.pdf"), pdf).unwrap();
        s.document
            .save(&std::path::Path::new(&dir).join("guide-pages.capture.json"))
            .unwrap();
    }
    let before = bytes(&s.document);
    for action in [
        Action::PageSize {
            width: 900,
            height: 200,
            auto_overflow: true,
        },
        Action::DeletePage { page: 99 },
        Action::SectionPage {
            id: 999,
            page: 0,
            copy: true,
        },
    ] {
        assert!(s.execute(Command::Guide { action }).is_err());
        assert_eq!(bytes(&s.document), before);
    }
    execute(
        &mut s,
        Action::PageSize {
            width: 450,
            height: 400,
            auto_overflow: true,
        },
    );
    assert_eq!(
        s.document
            .guide
            .as_ref()
            .unwrap()
            .page_width(s.document.width),
        450
    );
    s.document.validate().unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), before);
}

#[test]
fn legacy_photo_and_colored_templates_keep_page_backgrounds_without_stretching() {
    for photo in [false, true] {
        let image = image::RgbaImage::from_fn(160, 160, |x, y| {
            image::Rgba(if photo {
                [x as u8, y as u8, 100, 255]
            } else {
                [40, 80, 160, 255]
            })
        });
        let mut d = Document::from_image(image.clone()).unwrap();
        d.guide = Some(Layout {
            pages: None,
            template: Some("local".into()),
            top: 0.,
            sections: vec![Section {
                page: 0,
                id: 1,
                kind: SectionKind::Image,
                height: 160.,
                objects: vec![],
                marker: None,
            }],
        });
        d.promote_features();
        d.validate().unwrap();
        let next = d
            .edit_guide(&Action::AddPage {
                page: 0,
                copy: true,
            })
            .unwrap();
        let output = next.render().unwrap();
        for left in [0, 160 + PAGE_GAP] {
            assert_eq!(
                image::imageops::crop_imm(&output, left, 0, 160, 160).to_image(),
                image
            );
        }
        let mut session = Session::new(d.clone()).unwrap();
        execute(
            &mut session,
            Action::AddPage {
                page: 0,
                copy: true,
            },
        );
        session.execute(Command::Undo).unwrap();
        assert_eq!(bytes(&session.document), bytes(&d));
    }
}

#[test]
fn page_validation_rejects_missing_membership_and_locked_page_deletion() {
    let mut s = Session::new(template()).unwrap();
    execute(
        &mut s,
        Action::AddPage {
            page: 0,
            copy: false,
        },
    );
    let mut bad = s.document.clone();
    bad.guide.as_mut().unwrap().sections[0].page = 999;
    assert!(bad.validate().is_err());
    let id = s.document.annotations[0].id;
    let mut state = s.document.object_state(id);
    state.locked = true;
    s.document.object_states.insert(id, state);
    let before = bytes(&s.document);
    assert!(
        s.execute(Command::Guide {
            action: Action::DeletePage { page: 0 }
        })
        .is_err()
    );
    assert_eq!(bytes(&s.document), before);
}

#[test]
fn horizontal_page_reordering_and_overlap_ownership_keep_independent_objects() {
    let mut s = Session::new(template()).unwrap();
    execute(
        &mut s,
        Action::AddPage {
            page: 0,
            copy: true,
        },
    );
    execute(
        &mut s,
        Action::AddPage {
            page: 1,
            copy: false,
        },
    );
    let before = bytes(&s.document);
    let g = s.document.guide.as_ref().unwrap().clone();
    let object = s
        .document
        .annotations
        .iter()
        .find(|a| g.sections[0].objects.contains(&a.id))
        .unwrap()
        .clone();
    execute(&mut s, Action::MovePage { page: 0, index: 2 });
    let moved = s
        .document
        .annotations
        .iter()
        .find(|a| a.id == object.id)
        .unwrap();
    assert!((moved.start[0] - object.start[0] - g.page_offset(2)[0]).abs() < 0.01);
    assert_eq!(moved.start[1], object.start[1]);
    assert_eq!(moved.group, object.group);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), before);
    // Move a shape across a section boundary without grouping or explicit reassignment.
    let spans = g.spans();
    let boundary = spans[0].2;
    let mut a = object
        .converted_frame(false)
        .unwrap_or_else(|_| object.clone());
    a.id = 9999;
    a.kind = Kind::Rectangle;
    a.image = None;
    a.text = None;
    a.points.clear();
    a.callout = None;
    a.group = None;
    a.corner_radius = 0.;
    a.start = [40., boundary - 30.];
    a.end = [100., boundary + 10.];
    s.execute(Command::Add {
        annotation: a.clone(),
    })
    .unwrap();
    assert!(
        s.document.guide.as_ref().unwrap().sections[0]
            .objects
            .contains(&a.id)
    );
    a.start[1] += 25.;
    a.end[1] += 25.;
    s.execute(Command::Update {
        annotation: a.clone(),
    })
    .unwrap();
    let g = s.document.guide.as_ref().unwrap();
    assert!(!g.sections[0].objects.contains(&a.id));
    assert!(g.sections[1].objects.contains(&a.id));
    let id = g.sections[1].id;
    let before = bytes(&s.document);
    execute(
        &mut s,
        Action::SectionPage {
            id,
            page: 2,
            copy: false,
        },
    );
    let moved = s
        .document
        .annotations
        .iter()
        .find(|b| b.id == a.id)
        .unwrap();
    assert!(moved.start[0] > a.start[0]);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), before);
}

#[test]
fn unsupported_development_projects_are_rejected_without_rewriting_files() {
    let d = crate::Document::blank(40, 30).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.capture.json");
    for version in [1, 27, 28, 30, 32] {
        let mut old = d.clone();
        old.version = version;
        let bytes = serde_json::to_vec(&old).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(crate::Document::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
