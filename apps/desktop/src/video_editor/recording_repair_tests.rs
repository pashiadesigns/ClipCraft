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
fn hovering_a_placed_step_shows_selection_instead_of_the_next_step() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let mut step = e.session.document.annotations[1].clone();
    step.start = [370., 240.];
    step.end = [434., 304.];
    e.execute(Command::Update {
        annotation: step.clone(),
    });
    let p = e.session.document.video.as_mut().unwrap();
    let t = p.objects.get_mut(&step.id).unwrap();
    t.start = 4000;
    t.entrance = Animation::Float;
    t.entrance_ms = 2000;
    t.smooth = false;
    p.cursor = 4500;
    e.tool = Some(crate::Kind::Step);
    e.select_only(None);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let t = e.view.current_transform().unwrap();
    // Part way through its entrance the Step is drawn away from its final place.
    let (offset, scale) = e.video_object_motion(step.id);
    let drawn = [402. * scale + offset[0], 272. * scale + offset[1]];
    let on = t.screen_point(drawn);
    assert!(e.hover_selects(on, t));
    assert!(!e.hover_selects(t.screen_point([40., 40.]), t));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(on)]);
    let shown = render(&ctx, &mut e, vec![egui::Event::PointerMoved(on)]);
    assert_eq!(shown.0.platform_output.cursor_icon, egui::CursorIcon::Grab);
}

#[test]
fn finished_recording_opens_insert_and_timeline_instead_of_recent() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.history.recent = true;
    e.video.collapsed = true;
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Advanced;
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

fn two_steps(e: &mut Editor) -> (u64, u64) {
    let first = e.session.document.annotations[1].clone();
    assert_eq!(first.kind, crate::Kind::Step);
    let mut second = first.clone();
    second.id = 200;
    e.session.document.annotations.push(second);
    e.session
        .document
        .video
        .as_mut()
        .unwrap()
        .place(200, Timing::new(0, 8000, 9000));
    (first.id, 200)
}
#[test]
fn objects_selected_together_on_the_timeline_all_take_an_edit() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let (a, b) = two_steps(&mut e);
    e.timeline_focus(Item::Annotation(a), false);
    e.timeline_focus(Item::Annotation(b), true);
    assert_eq!(e.selected_objects().len(), 2);
    let mut edited = e.selected_annotation().unwrap();
    edited.color = [1, 2, 3, 255];
    e.preview_property_edit(edited);
    e.commit_property_preview();
    for id in [a, b] {
        let o = e.session.document.annotations.iter().find(|o| o.id == id);
        assert_eq!(o.unwrap().color, [1, 2, 3, 255], "object {id}");
    }
    let mut v = e.session.document.video.clone().unwrap();
    let old = v.objects[&a].clone();
    let before = v.objects[&b].clone();
    let mut after = before.clone();
    after.entrance = Animation::Fade;
    after.entrance_ms = 300;
    after.smooth = !before.smooth;
    after.start += 100;
    e.spread_timing(&mut v, b, &before, &after);
    let other = &v.objects[&a];
    assert_eq!(other.entrance, Animation::Fade);
    assert_eq!(other.entrance_ms, 300);
    assert_eq!(other.smooth, after.smooth);
    assert_eq!(other.start, old.start + 100, "the arrangement is kept");
    assert_eq!(other.end, old.end);
    // Ctrl+click takes one out and the other stays selected.
    e.timeline_focus(Item::Annotation(b), true);
    assert_eq!(e.selected_objects().len(), 1);
    assert_eq!(e.selected, Some(a));
}
#[test]
fn alt_drag_on_the_timeline_copies_zooms_and_whole_selections() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let (a, _) = two_steps(&mut e);
    let zoom = capture_core::video::zoom::Zoom::new(90, 1000, 2000);
    e.session.document.video.as_mut().unwrap().zooms.push(zoom);
    let start = e.session.document.clone();
    let items = std::collections::BTreeSet::from([Item::Zoom(90), Item::Annotation(a)]);
    let p = start.video.clone().unwrap();
    let old = p.objects[&a].clone();
    let moved = p.move_items_near(&items, 5000, 0).unwrap();
    e.commit_timeline_copies(&moved, &items);
    let v = e.session.document.video.clone().unwrap();
    assert_eq!(v.zooms.len(), p.zooms.len() + 1);
    assert_eq!(
        v.zooms[0],
        p.zooms[p.zooms.iter().position(|z| z.id == 90).unwrap()]
    );
    let copy = v.zooms.iter().find(|z| z.id != 90).unwrap();
    let dropped = moved.zooms.iter().find(|z| z.id == 90).unwrap();
    assert_eq!((copy.start, copy.end), (dropped.start, dropped.end));
    assert_eq!(copy.track(), p.zooms[0].track(), "room on the same track");
    assert_eq!(
        e.session.document.annotations.len(),
        start.annotations.len() + 1
    );
    assert_eq!(v.objects[&a].start, old.start, "the original stays");
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video, start.video);
    assert_eq!(e.session.document.annotations, start.annotations);
    let only = std::collections::BTreeSet::from([Item::Zoom(90)]);
    let moved = p.move_items_near(&only, 100, 0).unwrap();
    e.commit_timeline_copies(&moved, &only);
    let v = e.session.document.video.as_ref().unwrap();
    let copy = v.zooms.iter().find(|z| z.id != 90).unwrap();
    assert_ne!(
        copy.track(),
        v.zooms[0].track(),
        "an overlapping copy moves down"
    );
}
#[test]
fn alt_drag_gesture_leaves_the_zoom_and_drops_a_copy() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.set_timeline_expanded(true);
    let zoom = capture_core::video::zoom::Zoom::new(90, 1000, 2000);
    e.session.document.video.as_mut().unwrap().zooms.push(zoom);
    render(&ctx, &mut e, vec![]);
    let from = egui::pos2(400., 700.);
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(from)]);
    e.begin_timeline_duplicate(Item::Zoom(90), from);
    let to = from + egui::vec2(200., 0.);
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    assert!(e.video.copy_preview.is_some());
    assert!(e.video.timing_preview.is_none(), "the original stays put");
    render(&ctx, &mut e, press(to, false));
    let v = e.session.document.video.as_ref().unwrap();
    assert_eq!(v.zooms.len(), 2);
    assert_eq!(v.zooms.iter().find(|z| z.id == 90).unwrap().start, 1000);
}
#[test]
fn export_video_previews_the_watermark_on_the_playhead_frame() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.session.document.video.as_mut().unwrap().cursor = 4500;
    e.video.export_dialog = true;
    e.export_preferences.watermark_ready = true;
    let mark = capture_core::watermark::Watermark::default();
    e.export_preferences.options.watermark = Some(mark.clone());
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let preview = e.video.export_preview.as_ref().expect("a preview");
    assert!(preview.showing(&mark));
    e.export_preferences.options.watermark = None;
    render(&ctx, &mut e, vec![]);
}

#[test]
fn new_objects_start_with_the_motion_of_their_kind() {
    use capture_core::focus::Focus;
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let magnify = Focus {
        magnify: Some(Default::default()),
        ..Default::default()
    };
    let shapes = [
        (crate::Kind::Rectangle, None, Animation::Path),
        (crate::Kind::Ellipse, None, Animation::Path),
        (crate::Kind::Line, None, Animation::Path),
        (crate::Kind::Arrow, None, Animation::Path),
        (
            crate::Kind::Rectangle,
            Some(Focus::default()),
            Animation::Scale,
        ),
        (crate::Kind::Ellipse, Some(magnify), Animation::Scale),
    ];
    for (kind, focus, expected) in shapes {
        let mut a = e.new_annotation(kind, [100., 100.], [260., 220.]);
        a.focus = focus;
        let id = a.id;
        e.execute(Command::Add { annotation: a });
        let t = &e.session.document.video.as_ref().unwrap().objects[&id];
        assert_eq!((t.entrance, t.exit), (expected, expected), "{kind:?}");
    }
    let a = e.step_marker([300., 300.]).unwrap();
    let id = a.id;
    e.execute(Command::Add { annotation: a });
    let t = &e.session.document.video.as_ref().unwrap().objects[&id];
    assert_eq!((t.entrance, t.exit), (Animation::Float, Animation::Float));
    assert_eq!(t.direction, 0, "from the left");
    let a = e.new_annotation(crate::Kind::Text, [20., 20.], [200., 80.]);
    let id = a.id;
    e.execute(Command::Add { annotation: a });
    let t = &e.session.document.video.as_ref().unwrap().objects[&id];
    assert_eq!(t.entrance, Animation::None, "other kinds are unchanged");
}

fn all_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = vec![];
    shapes.iter().for_each(|c| walk(&c.shape, &mut out));
    out
}
fn step_shape_line(e: &mut Editor) -> [u64; 3] {
    let (step, shape) = two_steps(e);
    let mut rectangle = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.id == shape)
        .cloned()
        .unwrap();
    rectangle.kind = crate::Kind::Rectangle;
    rectangle.text = None;
    rectangle.step_outline = None;
    let mut line = rectangle.clone();
    line.id = 201;
    line.kind = crate::Kind::Line;
    *e.session
        .document
        .annotations
        .iter_mut()
        .find(|a| a.id == shape)
        .unwrap() = rectangle;
    e.session.document.annotations.push(line);
    e.session
        .document
        .video
        .as_mut()
        .unwrap()
        .place(201, Timing::new(0, 9000, 10000));
    [step, shape, 201]
}
#[test]
fn a_mixed_selection_shows_the_same_panel_from_the_timeline_or_the_canvas() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let [a, b, c] = step_shape_line(&mut e);
    e.video.tab = PropertyTab::Style;
    e.video.object_tab = PropertyTab::Style;
    e.timeline_focus(Item::Annotation(a), false);
    e.timeline_focus(Item::Annotation(b), true);
    e.timeline_focus(Item::Annotation(c), true);
    render(&ctx, &mut e, vec![]);
    let timeline = all_text(&render(&ctx, &mut e, vec![]).shapes);
    for expected in [
        "3 objects selected",
        "Mixed",
        "Step",
        "Rectangle",
        "Line",
        "Fill and outline",
    ] {
        assert!(timeline.iter().any(|t| t == expected), "{expected} missing");
    }
    e.select_only(None);
    e.clear_timeline_selection();
    render(&ctx, &mut e, vec![]);
    e.click_selection(Some(a), false);
    e.click_selection(Some(b), true);
    e.click_selection(Some(c), true);
    e.sync_timeline_with_canvas();
    render(&ctx, &mut e, vec![]);
    let canvas = all_text(&render(&ctx, &mut e, vec![]).shapes);
    let mut one = timeline.clone();
    let mut two = canvas.clone();
    one.sort();
    two.sort();
    assert_eq!(one, two, "the same panel either way");
    assert_eq!(
        e.video.selection.len(),
        3,
        "the canvas selection shows on the timeline"
    );
}
#[test]
fn a_change_reaches_every_kind_that_shares_the_setting_in_one_undo() {
    use capture_core::multi_selection as shared;
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let [a, b, c] = step_shape_line(&mut e);
    e.timeline_focus(Item::Annotation(a), false);
    e.timeline_focus(Item::Annotation(b), true);
    e.timeline_focus(Item::Annotation(c), true);
    let before = e.session.document.annotations.clone();
    let line_before = before.iter().find(|o| o.id == c).unwrap().clone();
    e.apply_mixed(|o| shared::set_fill(o, [1, 2, 3, 255]));
    e.commit_property_preview();
    e.apply_mixed(|o| shared::set_outline(o, [4, 5, 6, 255]));
    e.commit_property_preview();
    let find = |e: &Editor, id: u64| {
        e.session
            .document
            .annotations
            .iter()
            .find(|o| o.id == id)
            .unwrap()
            .clone()
    };
    assert_eq!(shared::fill(&find(&e, a)), [1, 2, 3, 255]);
    assert_eq!(shared::fill(&find(&e, b)), [1, 2, 3, 255]);
    assert_eq!(find(&e, c).fill, line_before.fill, "lines have no fill");
    for id in [a, b, c] {
        assert_eq!(
            shared::outline(&find(&e, id)),
            [4, 5, 6, 255],
            "object {id}"
        );
    }
    e.execute(Command::Undo);
    assert_eq!(shared::fill(&find(&e, a)), [1, 2, 3, 255]);
    assert_ne!(shared::outline(&find(&e, a)), [4, 5, 6, 255]);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before);
}
#[test]
fn text_settings_on_a_mixed_selection_keep_for_a_shape_with_no_text() {
    use capture_core::multi_selection as shared;
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let [a, b, _] = step_shape_line(&mut e);
    e.timeline_focus(Item::Annotation(a), false);
    e.timeline_focus(Item::Annotation(b), true);
    e.apply_mixed(|o| {
        let mut with_text = shared::with_room_for_text(o.clone());
        if let Some(t) = with_text.text.as_mut() {
            t.size_points = 33.;
        }
        *o = with_text;
    });
    e.commit_property_preview();
    let shape = e
        .session
        .document
        .annotations
        .iter()
        .find(|o| o.id == b)
        .unwrap();
    let text = shape.text.as_ref().expect("the setting is kept");
    assert_eq!((text.content.as_str(), text.size_points), ("", 33.));
    e.session.document.validate().unwrap();
    e.session.document.render().unwrap();
}
#[test]
fn switching_ribbon_tabs_draws_correct_buttons_on_the_first_frame() {
    fn fills(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::Color32> {
        fn walk(s: &egui::Shape, out: &mut Vec<egui::Color32>) {
            match s {
                egui::Shape::Rect(r) if r.rect.width() >= 90. && r.rect.height() >= 60. => {
                    out.push(r.fill)
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = vec![];
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
    for _ in 0..4 {
        render(&ctx, &mut e, vec![]);
    }
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Export;
    let first = fills(&render(&ctx, &mut e, vec![]).shapes);
    let settled = fills(&render(&ctx, &mut e, vec![]).shapes);
    assert_eq!(first, settled);
}
#[test]
fn copy_frame_uses_the_frame_on_screen_at_the_project_size() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.session.document.video.as_mut().unwrap().cursor = 4500;
    render(&ctx, &mut e, vec![]);
    e.video.frame_generation = e.video.generation;
    let size = [e.session.document.width, e.session.document.height];
    e.video.render_project = Some((e.session.document.video.clone().unwrap(), size));
    let mut copied = None;
    e.copy_all_with(|d| {
        copied = Some((d.width, d.height));
        Ok(())
    });
    assert_eq!(
        copied,
        Some((e.session.document.width, e.session.document.height)),
        "{}",
        e.status
    );
    assert!(e.status.contains("copied"), "{}", e.status);
}
#[test]
fn the_video_ribbon_has_no_advanced_tab_and_a_short_formatting_tab() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Advanced;
    let out = all_text(&render(&ctx, &mut e, vec![]).shapes);
    assert!(!out.iter().any(|t| t == "Advanced"));
    assert_eq!(e.shell.ribbon_tab, crate::ribbon::RibbonTab::Formatting);
    render(&ctx, &mut e, vec![]);
    let out = all_text(&render(&ctx, &mut e, vec![]).shapes);
    for kept in ["Alignment", "Distribution", "Arrange", "Grouping"] {
        assert!(out.iter().any(|t| t == kept), "{kept} missing");
    }
    for gone in ["Justification", "Groups", "Animations"] {
        assert!(!out.iter().any(|t| t == gone), "{gone} still shown");
    }
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Export;
    render(&ctx, &mut e, vec![]);
    let out = all_text(&render(&ctx, &mut e, vec![]).shapes);
    assert!(out.iter().any(|t| t == "Copy frame"));
}
#[test]
fn the_timeline_follows_the_playhead_when_asked() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.set_timeline_expanded(true);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    e.video.zoom = 8.;
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    assert_eq!(e.video.timeline_offset.x, 0.);
    e.video.follow_playhead = false;
    e.session.document.video.as_mut().unwrap().cursor = 11_000;
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    assert_eq!(e.video.timeline_offset.x, 0., "off leaves the view alone");
    e.video.follow_playhead = true;
    e.session.document.video.as_mut().unwrap().cursor = 11_500;
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    assert!(
        e.video.timeline_offset.x > 0.,
        "the view moved to the playhead"
    );
}
#[test]
fn a_free_corner_previews_while_held_and_commits_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = crate::Session::new(crate::Document::blank(1920, 1080).unwrap()).unwrap();
    let pixels = image::RgbaImage::from_pixel(1600, 900, image::Rgba([40, 90, 60, 255]));
    let bitmap = capture_core::bitmap::Bitmap::transient(std::sync::Arc::new(pixels)).unwrap();
    let mut a = std::sync::Arc::new(bitmap).annotation(90);
    a.kind = crate::Kind::Image;
    a.start = [100., 100.];
    a.end = [1700., 1000.];
    e.session.document.annotations.push(a);
    e.tool = None;
    e.select_only(Some(90));
    crate::perspective_ui::show_free_corners(&ctx, 90);
    for _ in 0..4 {
        render(&ctx, &mut e, vec![]);
    }
    let corner: egui::Rect = ctx
        .data(|d| d.get_temp(egui::Id::unique(("perspective-corner-rect", 0usize))))
        .expect("corner");
    let at = corner.center();
    let saved = |e: &Editor| {
        e.session.document.annotations[0]
            .image
            .as_ref()
            .unwrap()
            .perspective()
    };
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(at)]);
    render(&ctx, &mut e, press(at, true));
    for i in 1..8 {
        render(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(
                at + egui::vec2(i as f32 * 6., i as f32 * 4.),
            )],
        );
        assert!(e.shell.perspective_drag && e.display.uses_retained_scene());
        assert_eq!(saved(&e), None, "held, not saved");
        assert!(
            e.selected_annotation()
                .unwrap()
                .image
                .as_ref()
                .unwrap()
                .perspective()
                .is_some()
        );
    }
    render(&ctx, &mut e, press(at + egui::vec2(42., 28.), false));
    e.settle_property_preview(&ctx);
    assert!(saved(&e).is_some());
    e.execute(Command::Undo);
    assert_eq!(saved(&e), None, "one Undo for the whole drag");
}

#[test]
fn installed_font_search_accepts_click_and_typing_without_dismissing_popup() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let mut a = e.new_annotation(crate::Kind::Text, [20., 20.], [260., 90.]);
    a.text.get_or_insert_with(Default::default).content = "Font search".into();
    let id = a.id;
    e.execute(Command::Add { annotation: a });
    e.select_only(Some(id));
    e.tool = None;
    e.shell.properties = true;
    for _ in 0..3 {
        render(&ctx, &mut e, vec![]);
    }
    let rect_of = |key: &str| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(key)));
    let click = |e: &mut Editor, r: egui::Rect| {
        render(&ctx, e, vec![egui::Event::PointerMoved(r.center())]);
        render(&ctx, e, press(r.center(), true));
        render(&ctx, e, press(r.center(), false));
    };
    click(&mut e, rect_of("font-search-button").expect("font button"));
    render(&ctx, &mut e, vec![]);
    click(&mut e, rect_of("font-search-input").expect("search box"));
    render(&ctx, &mut e, vec![egui::Event::Text("Ubuntu".into())]);
    assert_eq!(
        ctx.data(|d| d.get_temp::<String>(egui::Id::unique("font-search")))
            .as_deref(),
        Some("Ubuntu")
    );
    ctx.data_mut(|d| d.remove::<egui::Rect>(egui::Id::unique("font-search-input")));
    render(&ctx, &mut e, vec![]);
    assert!(
        rect_of("font-search-input").is_some(),
        "the popup stays open"
    );
}

#[test]
fn holding_the_mouse_on_a_property_tab_leaves_the_pane_as_it_is() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let [a, b, c] = step_shape_line(&mut e);
    let selections: [&[u64]; 3] = [&[a], &[a, b], &[a, b, c]];
    for picked in selections {
        e.select_only(None);
        e.clear_timeline_selection();
        for (i, id) in picked.iter().enumerate() {
            e.timeline_focus(Item::Annotation(*id), i > 0);
        }
        for tab in [PropertyTab::Style, PropertyTab::Timing, PropertyTab::Motion] {
            e.video.tab = tab;
            e.video.object_tab = tab;
            for _ in 0..3 {
                render(&ctx, &mut e, vec![]);
            }
            let rect = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("video-tab", tab.label()))))
                .unwrap();
            let before = all_text(
                &render(&ctx, &mut e, vec![egui::Event::PointerMoved(rect.center())]).shapes,
            );
            let held = all_text(&render(&ctx, &mut e, press(rect.center(), true)).shapes);
            assert_eq!(before, held, "{picked:?} {}", tab.label());
            let later = all_text(&render(&ctx, &mut e, vec![]).shapes);
            assert_eq!(before, later, "{picked:?} {}", tab.label());
            render(&ctx, &mut e, press(rect.center(), false));
        }
    }
}
#[test]
fn float_can_come_from_any_side() {
    let names: Vec<_> = capture_core::reusable_presets::Motion::builtins()
        .into_iter()
        .filter(|(_, m)| m.entrance == Animation::Float)
        .map(|(n, m)| (n.to_owned(), m.direction))
        .collect();
    assert_eq!(
        names,
        [
            ("Float from left".to_owned(), 0),
            ("Float from right".to_owned(), 1),
            ("Float from top".to_owned(), 2),
            ("Float from bottom".to_owned(), 3)
        ]
    );
}

#[test]
fn an_edit_reaches_every_selected_clip_and_keeps_their_spacing() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    let mut second = p.segments[0].clone();
    second.id = 40;
    second.at = p.segments[0].until() + 1000;
    second.track = p.segments[0].track;
    p.segments.push(second);
    let first_id = p.segments[0].id;
    let second_at = p.segments[1].at;
    e.timeline_focus(Item::Media(first_id), false);
    e.timeline_focus(Item::Media(40), true);
    e.timeline_focus(Item::Media(first_id), false);
    let original = e.session.document.video.clone().unwrap();
    let mut v = original.clone();
    v.segments[0].fade_in = 300;
    v.segments[0].at += 250;
    v.segments[0]
        .frame
        .get_or_insert_with(Default::default)
        .radius = 33.;
    v.segments[1].frame = Some(Default::default());
    let mut base = original.clone();
    base.segments[0].frame = Some(Default::default());
    base.segments[1].frame = Some(Default::default());
    e.spread_clip_edit(&mut v, &base, 0);
    assert_eq!(v.segments[1].fade_in, 300);
    assert_eq!(v.segments[1].at, second_at + 250, "the spacing is kept");
    assert_eq!(v.segments[1].frame.as_ref().unwrap().radius, 33.);
}
#[test]
fn an_edit_reaches_every_selected_zoom_but_each_keeps_its_focus() {
    use capture_core::video::zoom::Zoom;
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    let mut far = Zoom::new(2, 5000, 6000);
    far.center = [0.8, 0.8];
    p.zooms.push(Zoom::new(1, 1000, 2000));
    p.zooms.push(far);
    e.timeline_focus(Item::Zoom(1), false);
    e.timeline_focus(Item::Zoom(2), true);
    e.timeline_focus(Item::Zoom(1), false);
    assert_eq!(e.video.zoom_selected, Some(1));
    let before = e.session.document.video.clone().unwrap();
    let mut v = before.clone();
    let z = v.zooms.iter_mut().find(|z| z.id == 1).unwrap();
    z.scale *= 0.5;
    z.start += 100;
    z.end += 100;
    let scale = z.scale;
    e.spread_zoom_edit(&mut v, &before, 1);
    let other = v.zooms.iter().find(|z| z.id == 2).unwrap();
    assert_eq!(other.scale, scale);
    assert_eq!((other.start, other.end), (5100, 6100));
    assert_eq!(other.center, [0.8, 0.8], "where it looks stays its own");
}
#[test]
fn alt_dragging_a_clip_frame_drops_a_copy_on_its_own_track() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let start = e.session.document.video.clone().unwrap();
    let (mut project, copy) = super::frames::duplicate_clip(&start, 0).expect("a copy");
    assert_eq!(
        project.segments.len(),
        start.segments.len()
            + 1
            + usize::from(start.segments[0].link.is_some() && start.segments.len() > 1)
    );
    assert_ne!(project.segments[copy].id, start.segments[0].id);
    assert_ne!(project.segments[copy].track, start.segments[0].track);
    project.segments[copy].rect = [0.1, 0.1, 0.5, 0.5];
    let saved = project.segments[0].clone();
    e.commit_video(project);
    assert!(!e.status.contains("Invalid"), "{}", e.status);
    let v = e.session.document.video.as_ref().unwrap();
    assert_eq!(v.segments[0], saved, "the original stays");
    assert_eq!(
        v.segments.len(),
        start.segments.len()
            + 1
            + usize::from(start.segments[0].link.is_some() && start.segments.len() > 1)
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video.as_ref(), Some(&start));
}
