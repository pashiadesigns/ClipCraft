//! Every named control explains itself in its single tooltip (bh-proposal).
use super::*;

/// Window chrome and read-outs that are not features with a Help topic.
const NO_TOPIC: [&str; 11] = [
    "ClipDesigner",
    "Close window",
    "Maximize window",
    "Minimize window",
    "Help",
    "Hide",
    "Plus",
    "Minus",
    "Close",
    "960 × 540",
    "Untitled — ClipDesigner",
];
fn assert_controls_have_topics() {
    let missing: Vec<String> = crate::tips::tests::NAMED
        .with(|u| u.borrow().clone())
        .into_iter()
        .filter(|n| crate::help::for_control(n).is_none() && !NO_TOPIC.contains(&n.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "controls with no Help topic: {missing:#?}"
    );
}

#[test]
fn every_tool_and_ribbon_control_names_itself_and_has_a_topic() {
    for video in [false, true] {
        crate::tips::tests::NAMED.with(|u| u.borrow_mut().clear());
        crate::tips::tests::UNNAMED.with(|u| u.borrow_mut().clear());
        let ctx = egui::Context::default();
        let mut e = repair_tests::editor(&ctx);
        if !video {
            let doc: capture_core::Document = serde_json::from_str(include_str!(
                "../../../../crates/core/tests/fixtures/properties.capture.json"
            ))
            .unwrap();
            e.session = Session::new(doc).unwrap();
        }
        use crate::ribbon::RibbonTab as R;
        for tab in [R::Insert, R::Formatting, R::Advanced, R::Export] {
            e.shell.ribbon_tab = tab;
            repair_tests::render(&ctx, &mut e, vec![]);
            repair_tests::render(&ctx, &mut e, vec![]);
        }
        // A read-out such as the zoom percentage is its own name.
        let unnamed: Vec<String> = crate::tips::tests::UNNAMED
            .with(|u| u.borrow().clone())
            .into_iter()
            .filter(|n| !n.ends_with('%'))
            .collect();
        let missing: Vec<String> = crate::tips::tests::NAMED
            .with(|u| u.borrow().clone())
            .into_iter()
            .filter(|n| crate::help::for_control(n).is_none() && !NO_TOPIC.contains(&n.as_str()))
            .collect();
        for n in &unnamed {
            eprintln!("UNNAMED {n}");
        }
        for n in &missing {
            eprintln!("NOTOPIC {n}");
        }
        assert!(
            unnamed.is_empty() && missing.is_empty(),
            "video={video}: {} unnamed, {} without a topic",
            unnamed.len(),
            missing.len()
        );
    }
}

#[test]
fn every_named_control_in_the_main_screens_has_an_explanation() {
    crate::tips::tests::UNEXPLAINED.with(|u| u.borrow_mut().clear());
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    for _ in 0..3 {
        repair_tests::render(&ctx, &mut e, vec![]);
    }
    let ids: Vec<u64> = e
        .session
        .document
        .annotations
        .iter()
        .map(|a| a.id)
        .collect();
    for id in ids {
        e.select_only(Some(id));
        for tab in [PropertyTab::Style, PropertyTab::Timing, PropertyTab::Motion] {
            e.video.tab = tab;
            e.video.object_tab = tab;
            repair_tests::render(&ctx, &mut e, vec![]);
        }
    }
    e.select_only(None);
    for kind in [
        crate::Kind::Rectangle,
        crate::Kind::Step,
        crate::Kind::Arrow,
        crate::Kind::Text,
    ] {
        e.tool = Some(kind);
        repair_tests::render(&ctx, &mut e, vec![]);
    }
    e.tool = None;
    e.open_video_capture();
    repair_tests::render(&ctx, &mut e, vec![]);
    e.clip.video_steps_tab = true;
    repair_tests::render(&ctx, &mut e, vec![]);
    assert_controls_have_topics();
    let missing = crate::tips::tests::UNEXPLAINED.with(|u| u.borrow().clone());
    assert!(
        missing.is_empty(),
        "controls without an explanation: {missing:#?}"
    );
}

#[test]
fn every_named_control_in_the_image_editor_has_an_explanation() {
    crate::tips::tests::UNEXPLAINED.with(|u| u.borrow_mut().clear());
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let doc: capture_core::Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    e.session = Session::new(doc).unwrap();
    for _ in 0..3 {
        repair_tests::render(&ctx, &mut e, vec![]);
    }
    let ids: Vec<u64> = e
        .session
        .document
        .annotations
        .iter()
        .map(|a| a.id)
        .collect();
    for id in ids {
        e.select_only(Some(id));
        repair_tests::render(&ctx, &mut e, vec![]);
        repair_tests::render(&ctx, &mut e, vec![]);
    }
    e.select_only(None);
    for kind in [
        crate::Kind::Rectangle,
        crate::Kind::Step,
        crate::Kind::Arrow,
        crate::Kind::Text,
        crate::Kind::Blur,
    ] {
        e.tool = Some(kind);
        repair_tests::render(&ctx, &mut e, vec![]);
    }
    assert_controls_have_topics();
    let missing = crate::tips::tests::UNEXPLAINED.with(|u| u.borrow().clone());
    assert!(
        missing.is_empty(),
        "controls without an explanation: {missing:#?}"
    );
}

#[test]
fn video_objects_with_text_get_a_text_tab_that_typing_opens() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    repair_tests::render(&ctx, &mut e, vec![]);
    let doc = &e.session.document;
    let shape = doc
        .annotations
        .iter()
        .find(|a| a.text.is_some() && a.kind != crate::Kind::Text)
        .map(|a| a.id);
    let text = doc
        .annotations
        .iter()
        .find(|a| a.kind == crate::Kind::Text)
        .map(|a| a.id);
    let Some(shape) = shape else {
        panic!("fixture needs an object that holds text");
    };
    e.select_only(Some(shape));
    repair_tests::render(&ctx, &mut e, vec![]);
    assert!(e.video_text_tab(), "a shape with text gets a Text tab");
    let tab = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("video-tab", "Text"))));
    assert!(tab.is_some(), "the Text tab is drawn after Style");
    let style = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("video-tab", "Style"))))
        .unwrap();
    assert!(tab.unwrap().left() > style.left());
    // Typing into the object opens Text; finishing returns to Style.
    e.video.tab = PropertyTab::Style;
    e.begin_text_edit();
    repair_tests::render(&ctx, &mut e, vec![]);
    assert_eq!(e.video.tab, PropertyTab::Text);
    e.text_draft = None;
    repair_tests::render(&ctx, &mut e, vec![]);
    assert_eq!(e.video.tab, PropertyTab::Style);
    // BH1: while typing, another tab can be chosen and stays chosen.
    e.begin_text_edit();
    repair_tests::render(&ctx, &mut e, vec![]);
    assert_eq!(e.video.tab, PropertyTab::Text);
    let style = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("video-tab", "Style"))))
        .unwrap();
    repair_tests::render(&ctx, &mut e, repair_tests::press(style.center(), true));
    repair_tests::render(&ctx, &mut e, repair_tests::press(style.center(), false));
    for _ in 0..3 {
        repair_tests::render(&ctx, &mut e, vec![]);
    }
    assert!(e.text_draft.is_some(), "choosing a tab keeps the text open");
    assert_eq!(
        e.video.tab,
        PropertyTab::Style,
        "Style stays open while typing"
    );
    e.text_draft = None;
    repair_tests::render(&ctx, &mut e, vec![]);
    assert_eq!(e.video.tab, PropertyTab::Style);
    if let Some(text) = text {
        e.select_only(Some(text));
        repair_tests::render(&ctx, &mut e, vec![]);
        assert!(!e.video_text_tab(), "Text objects keep one Style tab");
    }
}
