use super::repair_tests::{editor, press, render};
use super::*;
use capture_core::video::{
    editing::Item,
    editor::{Animation, Timing},
    links::Layer,
};

#[test]
fn large_step_carries_displayed_text_and_motion() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let mut a = e.session.document.annotations[1].clone();
    a.end = [a.start[0] + 240., a.start[1] + 240.];
    let text = a.text.as_mut().unwrap();
    let len = text.content.chars().count();
    capture_core::text::rich::format_range(text, 0..len, |f| {
        f.size = 28.;
        f.bold = true;
        f.color = Some([255, 90, 40, 255]);
    });
    e.execute(Command::Update {
        annotation: a.clone(),
    });
    e.restore_drawing_defaults(Some(crate::Kind::Step));
    let next = e.step_marker([500., 300.]).unwrap();
    assert_eq!(next.end[0] - next.start[0], 240.);
    let f = capture_core::text::rich::format_at(next.text.as_ref().unwrap(), 0);
    assert_eq!(f.size, 28.);
    assert!(f.bold);
    assert_eq!(f.color, Some([255, 90, 40, 255]));
}
#[test]
fn step_on_upper_track_wins_hit_and_moves_through_multiple_frames() {
    for motion in [Animation::Float, Animation::Scale] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let mut step = e.session.document.annotations[1].clone();
        if motion == Animation::Scale {
            step.kind = crate::Kind::Ellipse;
        }
        step.start = [370., 240.];
        step.end = [434., 304.];
        let mut behind = step.clone();
        behind.id = 100;
        behind.kind = crate::Kind::Rectangle;
        behind.text = None;
        behind.start = [0., 0.];
        behind.end = [960., 540.];
        behind.fill = Some([20, 30, 40, 255]);
        e.session.document.annotations = vec![step.clone(), behind];
        let p = e.session.document.video.as_mut().unwrap();
        p.objects.clear();
        p.tracks = vec![1, 2];
        p.layer_order = vec![Layer::Annotation(1), Layer::Annotation(2), Layer::Media(1)];
        p.place(step.id, Timing::new(1, 0, 12000));
        p.place(100, Timing::new(2, 0, 12000));
        let t = p.objects.get_mut(&step.id).unwrap();
        t.start = 4000;
        t.entrance = motion;
        t.entrance_ms = 2000;
        t.smooth = false;
        e.tool = None;
        e.select_only(None);
        let center = if motion == Animation::Float {
            [370., 270.]
        } else {
            [400., 270.]
        };
        assert_eq!(e.tool_hit(center, 3.), Some(step.id));
        render(&ctx, &mut e, vec![]);
        render(&ctx, &mut e, vec![]);
        let tr = e.view.current_transform().unwrap();
        let from = tr.screen_point(center);
        render(&ctx, &mut e, press(from, true));
        for dx in [25., 50., 75., 100.] {
            render(
                &ctx,
                &mut e,
                vec![egui::Event::PointerMoved(from + egui::vec2(dx, 0.))],
            );
            let moved = e.selected_annotation().unwrap();
            assert!(
                moved.start[0] > step.start[0] + dx / tr.scale - 8.,
                "motion={motion:?} dx={dx} {:?} {}",
                moved.start,
                e.status
            );
            assert!(e.video.frame_drag.is_none());
            assert!(
                (moved.start[0] - step.start[0] - dx / tr.scale).abs() < 8.,
                "motion={motion:?} {:?}",
                moved.start
            );
        }
        render(&ctx, &mut e, press(from + egui::vec2(100., 0.), false));
        assert!(e.session.document.annotations[0].start[0] > step.start[0] + 50.);
    }
}

#[test]
fn finished_recording_opens_insert_and_timeline_instead_of_recent() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.history.recent = true;
    e.video.collapsed = true;
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Clip;
    e.accept_capture_result(
        crate::capture::Captured {
            document: tests::fixture(),
            warning: "Recording ended at the last complete frame.".into(),
            retained: None,
            video: Some(crate::capture::VideoOutput {
                path: PathBuf::from("fixture.mp4"),
                preview: false,
                open_folder: false,
                seconds: 12,
            }),
        },
        capture_core::capture_options::Settings::default(),
        |_| Ok(()),
    );
    assert!(!e.video.collapsed);
    assert!(!e.history.recent);
    assert!(e.status.contains("last complete frame"));
    assert_eq!(e.shell.ribbon_tab, crate::ribbon::RibbonTab::Insert);
}
#[test]
fn delete_imported_and_mixed_selected_clips_is_one_undo() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let before = e.session.document.clone();
    e.timeline_focus(Item::Media(1), false);
    e.timeline_focus(Item::Annotation(2), true);
    assert!(e.has_video_selection());
    assert!(e.delete_video_selection());
    assert!(
        e.session
            .document
            .video
            .as_ref()
            .unwrap()
            .segments
            .is_empty()
    );
    assert!(!e.session.document.annotations.iter().any(|a| a.id == 2));
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video, before.video);
    assert_eq!(e.session.document.annotations, before.annotations);
    e.timeline_focus(Item::Media(1), false);
    e.click_selection(Some(2), false);
    assert!(e.delete_video_selection());
    assert_eq!(
        e.session.document.video.as_ref().unwrap().segments,
        before.video.as_ref().unwrap().segments
    );
    assert!(!e.session.document.annotations.iter().any(|a| a.id == 2));
}
#[test]
fn caret_formats_whole_block_unless_characters_are_selected() {
    let ctx = egui::Context::default();
    let mut e = crate::rich_text::tests::editor(&ctx);
    e.begin_text_edit();
    let d = e.text_draft.as_mut().unwrap();
    d.selection = 2..2;
    d.selection_active = true;
    let mut a = e.selected_annotation().unwrap();
    a.text.as_mut().unwrap().bold = true;
    e.update_text_sidebar(a, false);
    let t = e.selected_annotation().unwrap().text.unwrap();
    for i in 0..t.content.chars().count() {
        assert!(capture_core::text::rich::format_at(&t, i).bold);
    }
    e.text_draft.as_mut().unwrap().selection = 0..2;
    let mut a = e.selected_annotation().unwrap();
    a.text.as_mut().unwrap().italic = true;
    e.update_text_sidebar(a, false);
    let t = e.selected_annotation().unwrap().text.unwrap();
    assert!(capture_core::text::rich::format_at(&t, 0).italic);
    assert!(!capture_core::text::rich::format_at(&t, 3).italic);
}

#[test]
fn track_rename_replaces_selected_name_immediately() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.set_timeline_expanded(true);
    e.video.track_rename = Some((Layer::Media(1), "Old recording".into(), true));
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![egui::Event::Text("Narration".into())]);
    assert_eq!(e.video.track_rename.as_ref().unwrap().1, "Narration");
    render(
        &ctx,
        &mut e,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(e.video.track_rename.is_none());
    assert_eq!(
        e.session
            .document
            .video
            .as_ref()
            .unwrap()
            .custom_track_name(Layer::Media(1)),
        Some("Narration")
    );
}
#[test]
fn video_capture_preferences_default_and_persist_focus_policy() {
    let mut options = capture_core::video::Options::default();
    assert!(!options.pause_on_focus_loss);
    options.pause_on_focus_loss = true;
    let saved = serde_json::to_string(&options).unwrap();
    let restored: capture_core::video::Options = serde_json::from_str(&saved).unwrap();
    assert!(restored.pause_on_focus_loss);
}
#[cfg(windows)]
#[test]
fn preview_decoder_survives_audio_only_gap() {
    let _runtime = media::Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.mp4");
    let mut encoder = media::Encoder::new(&path, [320, 180], 30, false).unwrap();
    let pixels = vec![120u8; 320 * 180 * 4];
    for i in 0..30 {
        encoder.video(i * 10_000_000 / 30, &pixels).unwrap();
    }
    encoder.finish().unwrap();
    drop(encoder);
    let p = Project::new(path.to_string_lossy().into_owned(), [320, 180], 1000, 30);
    let mut visuals = media::Visuals::new();
    assert_eq!(visuals.sources(&p, 500).unwrap().len(), 1);
    assert!(visuals.sources(&p, 1500).unwrap().is_empty());
    assert_eq!(visuals.cached_decoders(), 1);
    assert_eq!(visuals.sources(&p, 500).unwrap().len(), 1);
}
