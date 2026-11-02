use super::*;
use crate::{Command, Session};
fn template() -> Document {
    crate::templates::build(
        "two-steps",
        "Setup fixture",
        &[],
        crate::templates::Scale::Fill,
    )
    .unwrap()
}
fn apply(s: &mut Session, settings: PageSetup, pages: Vec<usize>, scale: bool) {
    s.execute(Command::Guide {
        action: Action::PageSetup {
            settings: Box::new(settings),
            pages,
            scale_content: scale,
            auto_overflow: true,
            distribute: false,
        },
    })
    .unwrap();
}
#[test]
fn mixed_sizes_reorder_copy_delete_save_and_undo_preserve_settings_and_sources() {
    let d = template()
        .edit_guide(&Action::AddPage {
            page: 0,
            copy: true,
        })
        .unwrap();
    let original = serde_json::to_vec(&d).unwrap();
    let mut s = Session::new(d).unwrap();
    let setup = PageSetup {
        size: [1200, 1600],
        ppi: 150.,
        margins: [18.; 4],
        header: RunningText {
            enabled: true,
            text: "Operations guide".into(),
            ..Default::default()
        },
        numbers: true,
        ..Default::default()
    };
    apply(&mut s, setup.clone(), vec![1], true);
    let g = s.document.guide.as_ref().unwrap();
    assert_eq!(g.page_size(0, [0; 2]), [900, 948]);
    assert_eq!(g.page_size(1, [0; 2]), setup.size);
    assert_eq!(g.page_offset(1), [1060., 0.]);
    assert_eq!([s.document.width, s.document.height], [2260, 1600]);
    assert_eq!(g.page_setup(1), Some(&setup));
    assert_eq!(s.document.version, 30);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.capture.json");
    s.document.save(&path).unwrap();
    assert_eq!(
        serde_json::to_vec(&Document::load(&path).unwrap()).unwrap(),
        serde_json::to_vec(&s.document).unwrap()
    );
    for a in s
        .document
        .annotations
        .iter()
        .filter_map(|a| a.image.as_ref())
    {
        assert_eq!(a.size(), [240, 150]);
    }
    s.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&s.document).unwrap(), original);
    apply(&mut s, setup.clone(), vec![1], false);
    s.execute(Command::Guide {
        action: Action::MovePage { page: 1, index: 0 },
    })
    .unwrap();
    assert_eq!(
        s.document.guide.as_ref().unwrap().page_setup(0),
        Some(&setup)
    );
    s.execute(Command::Guide {
        action: Action::AddPage {
            page: 0,
            copy: true,
        },
    })
    .unwrap();
    assert_eq!(
        s.document.guide.as_ref().unwrap().page_setup(1),
        Some(&setup)
    );
    s.execute(Command::Guide {
        action: Action::DeletePage { page: 0 },
    })
    .unwrap();
    assert_eq!(
        s.document.guide.as_ref().unwrap().page_setup(0),
        Some(&setup)
    );
}
#[test]
fn physical_units_numbering_and_output_boxes_are_independent_of_pixels() {
    let mut s = PageSetup {
        size: [2550, 3300],
        ppi: 300.,
        numbers: true,
        bleed: 9.,
        crop_marks: true,
        ..Default::default()
    };
    assert_eq!(s.trim_points(), [612., 792.]);
    assert_eq!(s.output_points(), [666.5, 846.5]);
    assert_eq!(s.number(1, 5), "Page 2 of 5");
    s.number_style = NumberStyle::Roman;
    assert_eq!(s.number(3, 5), "IV");
    s.number_style = NumberStyle::Letters;
    assert_eq!(s.number(26, 30), "AA");
    s.mark_offset = 2.;
    assert!(s.validate().is_err());
    s.mark_offset = 9.;
    s.margins = [400.; 4];
    assert!(s.validate().is_err());
    s.margins = [0.; 4];
    s.ppi = f64::NAN;
    assert!(s.validate().is_err());
}
#[test]
fn exact_a4_print_size_survives_pixel_rounding_and_roundtrip() {
    let points = [210. * 72. / 25.4, 297. * 72. / 25.4];
    let setup = PageSetup {
        size: points.map(|n: f64| (n * 96. / 72.).round() as u32),
        physical_points: Some(points),
        margins: [0.; 4],
        ..Default::default()
    };
    setup.validate().unwrap();
    assert_eq!(setup.size, [794, 1123]);
    assert_eq!(setup.trim_points(), points);
    let mut s = Session::new(Document::blank(400, 300).unwrap()).unwrap();
    apply(&mut s, setup, vec![0], false);
    let bytes = serde_json::to_vec(&s.document).unwrap();
    let restored: Document = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    let pages = crate::export::PageSettings::default()
        .document_pages(&restored)
        .unwrap();
    assert_eq!(pages[0].media_size, points);
    let pdf = crate::export::encode_document(
        &restored,
        &crate::export::Options {
            format: crate::export::Format::Pdf,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        String::from_utf8_lossy(&pdf)
            .contains(&format!("/MediaBox [0 0 {} {}]", points[0], points[1]))
    );
}
#[test]
fn applying_to_plain_capture_promotes_the_base_to_an_editable_object_once() {
    let image = image::RgbaImage::from_fn(300, 240, |x, _| image::Rgba([x as u8, 120, 200, 255]));
    let original = Document::from_image(image).unwrap();
    let mut session = Session::new(original.clone()).unwrap();
    apply(&mut session, PageSetup::legacy([400, 500]), vec![0], false);
    assert_eq!(session.document.annotations.len(), 1);
    assert!(session.document.annotations[0].image.is_some());
    apply(&mut session, PageSetup::legacy([450, 550]), vec![0], false);
    assert_eq!(session.document.annotations.len(), 1);
    session.execute(Command::Undo).unwrap();
    session.execute(Command::Undo).unwrap();
    assert_eq!(
        serde_json::to_vec(&session.document).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
}
#[test]
fn layout_margins_reflow_whole_sections_and_keep_geometry_when_not_scaling() {
    let mut session = Session::new(template()).unwrap();
    let old = session.document.annotations[0].clone();
    apply(
        &mut session,
        PageSetup {
            size: [1100, 700],
            margins: [36.; 4],
            ..Default::default()
        },
        vec![0],
        false,
    );
    let g = session.document.guide.as_ref().unwrap();
    assert!(g.page_count() > 1);
    for (i, (_, _, bottom)) in g.spans().iter().enumerate() {
        assert!(*bottom <= g.content_bottom(g.sections[i].page) + 0.01);
    }
    let a = &session.document.annotations[0];
    assert_eq!(a.end[0] - a.start[0], old.end[0] - old.start[0]);
    assert_eq!(a.start[0] - old.start[0], 48.);
    let bytes = serde_json::to_vec(&session.document).unwrap();
    session.document.object_states.insert(
        a.id,
        crate::object_state::ObjectState {
            locked: true,
            ..Default::default()
        },
    );
    let result = session.document.edit_guide(&Action::PageSetup {
        settings: Box::new(PageSetup {
            margins: [72.; 4],
            ..Default::default()
        }),
        pages: vec![0],
        scale_content: true,
        auto_overflow: true,
        distribute: false,
    });
    assert!(result.is_err());
    assert!(!bytes.is_empty());
}

#[test]
fn mixed_print_sizes_furniture_and_bleed_have_matching_raster_and_vector_outputs() {
    let mut session = Session::new(
        template()
            .edit_guide(&Action::AddPage {
                page: 0,
                copy: true,
            })
            .unwrap(),
    )
    .unwrap();
    let mut setup = PageSetup {
        size: [1020, 1320],
        ppi: 120.,
        margins: [18.; 4],
        numbers: true,
        bleed: 9.,
        crop_marks: true,
        ..Default::default()
    };
    setup.header = RunningText {
        enabled: true,
        text: "Operations guide".into(),
        distance: 12.,
        ..Default::default()
    };
    setup.footer = RunningText {
        enabled: true,
        text: "Internal reference".into(),
        distance: 12.,
        ..Default::default()
    };
    apply(&mut session, setup.clone(), vec![0], true);
    setup.size = [1260, 1020];
    setup.ppi = 120.;
    setup.header.alignment = Alignment::Center;
    apply(&mut session, setup, vec![1], false);
    let doc = &session.document;
    let pages = crate::export::PageSettings::default()
        .document_pages(doc)
        .unwrap();
    assert_eq!(pages[0].destination[2..], [612., 792.]);
    assert_eq!(pages[1].destination[2..], [756., 612.]);
    assert_eq!(doc.page_furniture().len(), 6);
    let options = crate::export::Options {
        format: crate::export::Format::Pdf,
        ..Default::default()
    };
    let pdf = crate::export::encode_document(doc, &options).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert_eq!(raw.matches("/TrimBox").count(), 2);
    assert_eq!(raw.matches("/BleedBox").count(), 2);
    for p in &pages {
        let pixels = p.print_pixels(doc).unwrap();
        assert_eq!(pixels.width(), (p.media_size[0] * 120. / 72.).ceil() as u32);
    }
    if let Ok(dir) = std::env::var("CLIPCRAFT_PAGE_SETUP_FIXTURES") {
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        doc.save(&dir.join("page-setup-mixed.capture.json"))
            .unwrap();
        std::fs::write(dir.join("page-setup-mixed.pdf"), pdf).unwrap();
        for (i, p) in pages.iter().enumerate() {
            p.print_pixels(doc)
                .unwrap()
                .save(dir.join(format!("page-setup-print-{}.png", i + 1)))
                .unwrap();
        }
    }
}
