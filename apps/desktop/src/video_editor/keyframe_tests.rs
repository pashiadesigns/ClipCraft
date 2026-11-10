use super::*;
#[test]
fn the_keyframes_switch_opens_motion_and_returns_to_style() {
    let ctx = egui::Context::default();
    let mut e = super::repair_tests::editor(&ctx);
    e.video.tab = PropertyTab::Timing;
    e.keyframes_tab(true);
    assert!(e.video.tab == PropertyTab::Timing, "nothing selected");
    let id = e.session.document.annotations[0].id;
    e.select_only(Some(id));
    e.keyframes_tab(true);
    assert!(e.video.tab == PropertyTab::Motion && e.video.object_tab == PropertyTab::Motion);
    e.keyframes_tab(false);
    assert!(e.video.tab == PropertyTab::Style && e.video.object_tab == PropertyTab::Style);
    e.select_only(None);
    e.video.selected = Selection::Recording(0);
    e.keyframes_tab(true);
    assert!(e.video.tab == PropertyTab::Motion);
}
use capture_core::video::keyframes::{Property, Value};
#[test]
fn timeline_diamond_retimes_one_key_and_undo_restores_it() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    let t = e
        .session
        .document
        .video
        .as_mut()
        .unwrap()
        .objects
        .get_mut(&2)
        .unwrap();
    t.keyframes.insert(
        Property::Position,
        1000,
        Value::Numbers(vec![20., 20.]),
        Value::Numbers(vec![40., 40.]),
    );
    let original = e.session.document.video.clone().unwrap();
    let rect = egui::Rect::from_min_size(egui::pos2(40., 40.), egui::vec2(600., 35.));
    let draw = |e: &mut Editor, events| {
        let p = e
            .video
            .timing_preview
            .clone()
            .unwrap_or_else(|| e.session.document.video.clone().unwrap());
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 200.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                e.keyframe_marks(
                    ui,
                    &p,
                    capture_core::video::editing::Item::Annotation(2),
                    rect,
                    0.1,
                    None,
                );
            },
        );
        output.textures_delta.clear();
    };
    draw(&mut e, vec![]);
    let from = egui::pos2(140., 69.);
    let to = egui::pos2(240., 69.);
    draw(&mut e, repair_tests::press(from, true));
    draw(&mut e, vec![egui::Event::PointerMoved(to)]);
    draw(&mut e, repair_tests::press(to, false));
    let set = &e.session.document.video.as_ref().unwrap().objects[&2].keyframes;
    assert_eq!(set.channels[&Property::Position].keys[1].at, 2000);
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&2].keyframes,
        original.objects[&2].keyframes
    );
}
#[test]
fn recorded_cursor_shows_keys_follows_reduction_and_retimes_with_undo() {
    use capture_core::video::motion_path::{Input, Node, Path, Purpose, Reduction};
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    let mut path = Path {
        purpose: Purpose::Cursor,
        reference: [0., 0., 24., 24.],
        nodes: (0..=30)
            .map(|i| Node {
                at: i * 100,
                rect: [i as f32 * 7., (i % 4) as f32 * 9., 24., 24.],
                visible: true,
                sprite: 0,
                confidence: 1.,
            })
            .collect(),
        sprites: vec![],
        smooth: false,
        enabled: true,
        offset_ms: 0,
        reviewed: true,
        reduction: Reduction::Off,
        inputs: vec![
            Input {
                at: 1000,
                dragging: false,
                release: None,
            },
            Input {
                at: 2000,
                dragging: false,
                release: None,
            },
        ],
        recorded: None,
    };
    // Short lead so the hold keys (at click − 200 ms) leave room to retime.
    path.set_reduction(Reduction::Clicks(Reduction::MIN_LEAD));
    assert_eq!(
        path.nodes.iter().map(|n| n.at).collect::<Vec<_>>(),
        vec![0, 800, 1000, 1800, 2000, 2800, 3000]
    );
    let t = e
        .session
        .document
        .video
        .as_mut()
        .unwrap()
        .objects
        .get_mut(&2)
        .unwrap();
    t.motion_path = Some(std::sync::Arc::new(path));
    // The cursor path gets its own key lane even without property keys.
    e.video.keyframes.lanes = true;
    e.select_only(Some(2));
    let p = e.session.document.video.clone().unwrap();
    e.keyframe_rows(&p);
    assert_eq!(e.video.keyframes.rows.map(|r| r.1), Some(1));
    let before = e.session.document.video.clone().unwrap();
    let rect = egui::Rect::from_min_size(egui::pos2(40., 40.), egui::vec2(600., 35.));
    let draw = |e: &mut Editor, events| {
        let p = e
            .video
            .timing_preview
            .clone()
            .unwrap_or_else(|| e.session.document.video.clone().unwrap());
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 200.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                e.path_marks(ui, &p, 2, rect, 0.1);
            },
        );
        output.textures_delta.clear();
    };
    draw(&mut e, vec![]);
    let from = egui::pos2(140., 69.);
    let to = egui::pos2(190., 69.);
    draw(&mut e, repair_tests::press(from, true));
    draw(&mut e, vec![egui::Event::PointerMoved(to)]);
    draw(&mut e, repair_tests::press(to, false));
    let nodes = |e: &Editor| {
        e.session.document.video.as_ref().unwrap().objects[&2]
            .motion_path
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .map(|n| n.at)
            .collect::<Vec<_>>()
    };
    assert_eq!(nodes(&e), vec![0, 800, 1500, 1800, 2000, 2800, 3000]);
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&2].motion_path,
        before.objects[&2].motion_path
    );
}
#[test]
fn split_parts_show_only_their_own_path_keys() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    // Object 2 runs 4000–8000 ms; keys at ages 0, 1000, 3000.
    let t = e
        .session
        .document
        .video
        .as_mut()
        .unwrap()
        .objects
        .get_mut(&2)
        .unwrap();
    for (age, x) in [(1000, 300.), (3000, 600.)] {
        t.keyframes.insert(
            Property::Position,
            age,
            Value::Numbers(vec![100., 100.]),
            Value::Numbers(vec![x, 100.]),
        );
    }
    e.select_only(Some(2));
    assert_eq!(e.keyframe_path_points().len(), 3);
    let mut doc = e.session.document.clone();
    let right = capture_core::video::split::split_object(&mut doc, 2, 6000).unwrap();
    e.execute(Command::VideoComposition {
        project: doc.video.unwrap(),
        annotations: doc.annotations,
    });
    // Left part (ages 0–2000) shows keys 0 and 1000; right part shows only 3000.
    assert_eq!(e.keyframe_path_points().len(), 2);
    e.select_only(Some(right));
    assert_eq!(e.keyframe_path_points().len(), 1);
    // The keys are kept, so extending a part brings them back.
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(
        p.objects[&2].keyframes.channels[&Property::Position]
            .keys
            .len(),
        3
    );
}
#[test]
fn video_text_rotation_handle_keeps_selection_and_rotates() {
    for auto_key in [false, true] {
        let ctx = egui::Context::default();
        let mut e = repair_tests::editor(&ctx);
        e.video.keyframes.auto_key = auto_key;
        e.select_only(Some(3));
        repair_tests::render(&ctx, &mut e, vec![]);
        repair_tests::render(&ctx, &mut e, vec![]);
        let t = e.view.current_transform().unwrap();
        let a = e.selected_annotation().unwrap();
        let (min, max) = a.body_bounds();
        let handle = t.screen_point(a.rotation().handle(min, max, t.scale).unwrap());
        let center = t.screen_point(a.rotation().center);
        let to = center + (handle - center).rot90();
        assert!(
            e.rotation_hit(handle, t),
            "handle {handle:?} not hit; viewport {:?}",
            t.viewport
        );
        repair_tests::render(&ctx, &mut e, vec![egui::Event::PointerMoved(handle)]);
        assert_eq!(e.selected, Some(3), "hover lost selection");
        repair_tests::render(&ctx, &mut e, repair_tests::press(handle, true));
        assert!(
            e.rotation_drag.is_some(),
            "auto_key {auto_key}: rotation did not start; selected {:?}, draft {}",
            e.selected,
            e.text_draft.is_some()
        );
        repair_tests::render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
        repair_tests::render(&ctx, &mut e, repair_tests::press(to, false));
        assert_eq!(e.selected, Some(3), "auto_key {auto_key}: selection lost");
        let shown = e.selected_annotation().unwrap();
        assert!(
            (shown.rotation_degrees.abs() - 90.).abs() < 1.,
            "auto_key {auto_key}: rotation {}",
            shown.rotation_degrees
        );
        // AZ5: with Keyframes on, handle rotation is a Rotation key; the base keeps its angle.
        let p = e.session.document.video.as_ref().unwrap();
        let keyed = p.objects[&3]
            .keyframes
            .channels
            .contains_key(&Property::Rotation);
        let base = e
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == 3)
            .unwrap();
        if auto_key {
            assert!(keyed, "rotation did not create a key");
            assert_eq!(base.rotation_degrees, 0.);
        } else {
            assert!(!keyed);
        }
    }
}
#[test]
fn video_shape_corner_handle_resizes_instead_of_selecting_footage() {
    for auto_key in [false, true] {
        let ctx = egui::Context::default();
        let mut e = repair_tests::editor(&ctx);
        let a = e
            .session
            .document
            .annotations
            .iter_mut()
            .find(|a| a.id == 2)
            .unwrap();
        a.kind = crate::Kind::Rectangle;
        a.text = None;
        e.video.keyframes.auto_key = auto_key;
        e.select_only(Some(2));
        repair_tests::render(&ctx, &mut e, vec![]);
        repair_tests::render(&ctx, &mut e, vec![]);
        let t = e.view.current_transform().unwrap();
        let before = e.selected_annotation().unwrap();
        let (_, hi) = before.body_bounds();
        let corner = t.screen_point(hi);
        let to = corner + egui::vec2(40., 30.);
        assert!(
            before
                .hit_handle(t.image_point(corner), 7. / t.scale)
                .is_some(),
            "corner {corner:?} is not a handle"
        );
        repair_tests::render(&ctx, &mut e, repair_tests::press(corner, true));
        for step in 1..=4 {
            let p = corner + (to - corner) * step as f32 / 4.;
            repair_tests::render(&ctx, &mut e, vec![egui::Event::PointerMoved(p)]);
        }
        let dragged = ctx.dragged_id();
        let layer = ctx.layer_id_at(to);
        assert!(
            e.edit_drag.is_some(),
            "auto_key {auto_key}: no edit drag; dragged {dragged:?} layer {layer:?}; frame drag {}, marquee {}, group {}, rotate {}, zoom focus {}, timeline {}, status {:?}",
            e.video.frame_drag.is_some(),
            e.marquee.is_some(),
            e.group_drag.is_some(),
            e.rotation_drag.is_some(),
            e.video.zoom_focus_drag.is_some(),
            e.video.multi_drag.is_some(),
            e.status
        );
        repair_tests::render(&ctx, &mut e, repair_tests::press(to, false));
        assert_eq!(
            e.selected,
            Some(2),
            "auto_key {auto_key}: selection moved to footage"
        );
        assert_eq!(e.video.selected, Selection::Objects);
        let after = e.selected_annotation().unwrap();
        let grew = after.body_bounds().1[0] - before.body_bounds().1[0];
        assert!(grew > 10., "auto_key {auto_key}: corner moved {grew}");
    }
}
#[test]
fn keyframes_render_inside_full_video_workspace() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
    e.shell.properties = true;
    e.shell.backstage = None;
    e.shell.library = false;
    e.select_only(Some(2));
    e.video.tab = PropertyTab::Motion;
    e.video.object_tab = PropertyTab::Motion;
    e.video.keyframes.auto_key = true;
    e.video.keyframes.lanes = true;
    let t = e
        .session
        .document
        .video
        .as_mut()
        .unwrap()
        .objects
        .get_mut(&2)
        .unwrap();
    t.keyframes.insert(
        Property::Position,
        1500,
        Value::Numbers(vec![220., 100.]),
        Value::Numbers(vec![320., 180.]),
    );
    t.keyframes.insert(
        Property::Opacity,
        1500,
        Value::Numbers(vec![0.3]),
        Value::Numbers(vec![1.]),
    );
    let mut first = repair_tests::render(&ctx, &mut e, vec![]);
    for _ in 0..5 {
        let out = repair_tests::render(&ctx, &mut e, vec![]);
        first.0.textures_delta.append(out.0.textures_delta.clone());
    }
    let last = repair_tests::render(&ctx, &mut e, vec![]);
    assert_eq!(e.video.keyframes.rows.unwrap().1, 2);
    if let Some(folder) = std::env::var_os("CLIPCRAFT_GIF_EVIDENCE") {
        crate::property_layout::tests::dump_meshes(
            &ctx,
            &folder.to_string_lossy(),
            "keyframes-workspace",
            first,
            last,
        );
    }
}
#[test]
fn expanded_lanes_are_beneath_selection_and_drag_targets_stay_aligned() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    e.select_only(Some(2));
    e.video.keyframes.lanes = true;
    let p = e.session.document.video.as_mut().unwrap();
    p.objects.get_mut(&2).unwrap().keyframes.insert(
        Property::Rotation,
        1000,
        Value::Numbers(vec![0.]),
        Value::Numbers(vec![60.]),
    );
    let p = p.clone();
    e.keyframe_rows(&p);
    let (row, count) = e.video.keyframes.rows.unwrap();
    assert_eq!(count, 1);
    for i in 0..p.layers().len() {
        let y = keyframes::row_y(20., i, e.video.keyframes.rows);
        assert_eq!(keyframes::row_at(20., y + 10., e.video.keyframes.rows), i);
        if i == row + 1 {
            assert_eq!(y, 20. + i as f32 * 39. + 25.);
        }
    }
    // Dragging the keyed clip to another track keeps the lanes where the drag
    // began, so the rows under the pointer don't move while the mouse is down.
    let mut moved = p.clone();
    let other = moved
        .layers()
        .into_iter()
        .find_map(|l| match l {
            capture_core::video::links::Layer::Annotation(t) if t != moved.objects[&2].track => {
                Some(t)
            }
            _ => None,
        })
        .unwrap_or_else(|| moved.tracks.iter().max().unwrap() + 1);
    if !moved.tracks.contains(&other) {
        moved.tracks.push(other);
    }
    moved.objects.get_mut(&2).unwrap().track = other;
    let new_row = moved.layer_index(capture_core::video::links::Layer::Annotation(other));
    assert_ne!(new_row, row);
    e.video.track_drag = Some(capture_core::video::links::Layer::Annotation(other));
    e.keyframe_rows(&moved);
    assert_eq!(e.video.keyframes.rows, Some((row, count)));
    e.video.track_drag = None;
    e.keyframe_rows(&moved);
    assert_eq!(e.video.keyframes.rows, Some((new_row, count)));
}
#[cfg(windows)]
#[test]
fn keyed_video_frame_cpu_export_and_retained_preview_sample_identically() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("source.png");
    image::RgbaImage::from_pixel(160, 96, image::Rgba([80, 180, 210, 255]))
        .save(&file)
        .unwrap();
    let mut d = Document::blank(160, 96).unwrap();
    let mut p = Project::new(file.to_string_lossy().into_owned(), [160, 96], 2000, 30);
    p.segments[0].source = Some(capture_core::video::editor::Source {
        path: file.to_string_lossy().into_owned(),
        size: [160, 96],
        duration: 2000,
        kind: capture_core::video::editor::MediaKind::Image,
    });
    let k = &mut p.segments[0].keyframes;
    k.insert(
        Property::Size,
        0,
        Value::Numbers(vec![1., 1.]),
        Value::Numbers(vec![0.5, 0.5]),
    );
    k.insert(
        Property::Position,
        1000,
        Value::Numbers(vec![0., 0.]),
        Value::Numbers(vec![0.5, 0.5]),
    );
    d.video = Some(p.clone());
    d.promote_features();
    d.validate().unwrap();
    let _runtime = media::Runtime::new().unwrap();
    let mut reader = media::Visuals::new();
    for at in [0, 500, 1000, 1500] {
        let layers = reader.layers(&p, [160, 96], at).unwrap();
        let export = capture_core::video::frame::composite(&d, &layers, at).unwrap();
        let sources = reader.sources(&p, at).unwrap();
        let objects = capture_core::video::frame::source_objects(&d, &sources, at, false);
        let mut preview = image::RgbaImage::from_pixel(160, 96, image::Rgba(p.background));
        for object in objects {
            let pixels = capture_core::display::render_annotation_preview(
                &object,
                capture_core::display::Viewport {
                    size: [160, 96],
                    origin: [0.; 2],
                    scale: 1.,
                },
            )
            .unwrap();
            image::imageops::overlay(&mut preview, &pixels, 0, 0);
        }
        let differences: Vec<_> = preview
            .enumerate_pixels()
            .filter(|(x, y, pixel)| pixel != &export.get_pixel(*x, *y))
            .take(8)
            .map(|(x, y, pixel)| (x, y, *pixel, *export.get_pixel(x, y)))
            .collect();
        assert!(differences.is_empty(), "At {at}: {differences:?}");
    }
}
#[test]
fn live_key_preview_is_atomic_and_matches_committed_render() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    e.select_only(Some(2));
    e.video.keyframes.auto_key = true;
    let original = e.session.document.clone();
    let a = e.selected_annotation().unwrap();
    for x in [20., 45., 80.] {
        let mut edited = a.translated([x, 0.], [960, 540]).unwrap();
        edited.text.as_mut().unwrap().bold = true;
        e.preview_property_batch(vec![edited.clone()]);
        let doc = e.preview_document();
        let p = doc.video.as_ref().unwrap();
        let rendered =
            capture_core::video::editor::animated(&doc.annotations[1], &p.objects[&2], p.cursor)
                .unwrap();
        assert_eq!(rendered, edited);
        assert_eq!(e.session.document.annotations, original.annotations);
    }
    e.commit_property_preview();
    assert!(
        e.session.document.video.as_ref().unwrap().objects[&2]
            .keyframes
            .channels
            .contains_key(&Property::Text)
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, original.annotations);
    assert!(
        e.session.document.video.as_ref().unwrap().objects[&2]
            .keyframes
            .is_empty()
    );
}
#[test]
fn auto_keys_preview_commit_undo_and_seek_do_not_destroy_base() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    e.select_only(Some(2));
    e.video.keyframes.auto_key = true;
    let base = e.selected_annotation().unwrap();
    let original = e.session.document.clone();
    let moved = base.translated([80., 20.], [960, 540]).unwrap();
    e.execute(Command::Update {
        annotation: moved.clone(),
    });
    assert_eq!(e.selected_annotation().unwrap(), moved);
    assert_eq!(
        e.session
            .document
            .annotations
            .iter()
            .find(|a| a.id == 2)
            .unwrap(),
        &base
    );
    e.video_seek(4000);
    assert_eq!(e.selected_annotation().unwrap(), base);
    e.video_seek(4500);
    assert_eq!(e.selected_annotation().unwrap(), moved);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, original.annotations);
    assert!(
        e.session.document.video.as_ref().unwrap().objects[&2]
            .keyframes
            .is_empty()
    );
}
#[test]
fn motion_controls_lead_keyframes_follow_and_effects_keep_pane_size() {
    use capture_core::video::editor::Animation;
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    // A rectangle: after Fade + Scale, Path can still be chained, so the
    // Add animation row stays and each added effect adds exactly one row.
    let a = e
        .session
        .document
        .annotations
        .iter_mut()
        .find(|a| a.id == 2)
        .unwrap();
    a.kind = crate::Kind::Rectangle;
    a.text = None;
    e.select_only(Some(2));
    e.video.tab = PropertyTab::Motion;
    e.video.object_tab = PropertyTab::Motion;
    let measure = |e: &mut Editor| {
        let mut last = None;
        for _ in 0..4 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(340., 2400.),
                    )),
                    ..Default::default()
                },
                |ui| e.video_properties_pane(ui),
            );
            out.textures_delta.clear();
            last = Some(out);
        }
        let out = last.unwrap();
        let mut entrance = None;
        let mut right = 0f32;
        for shape in &out.shapes {
            if let egui::Shape::Text(t) = &shape.shape {
                if t.galley.text() == "Entrance" {
                    entrance = Some(t.pos.y);
                }
                right = right.max(t.pos.x + t.galley.size().x);
            }
        }
        let (keys, open) = ctx
            .data(|d| {
                d.get_temp::<(egui::Rect, bool)>(egui::Id::unique((
                    "property-group-state",
                    "Keyframes",
                )))
            })
            .expect("Keyframes section drawn");
        (entrance.expect("Entrance drawn"), keys, open, right)
    };
    let (entrance, keys, _, _) = measure(&mut e);
    assert!(
        entrance < keys.top(),
        "motion controls must come before keyframes"
    );
    // Turning keyframing on opens the section.
    e.video.keyframes.auto_key = true;
    e.video.keyframes.expand = true;
    let (_, _, open, _) = measure(&mut e);
    assert!(open, "keyframes open when keyframing turns on");
    let edit = |e: &mut Editor, stack: Vec<Animation>| {
        let t = e
            .session
            .document
            .video
            .as_mut()
            .unwrap()
            .objects
            .get_mut(&2)
            .unwrap();
        t.entrance = Animation::Fade;
        t.paired = false;
        t.entrance_stack = stack;
    };
    // Entrance starts collapsed (BA9); open it to measure its rows.
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(("property-subgroup", "Entrance")), true));
    edit(&mut e, vec![]);
    let (_, keys0, _, right0) = measure(&mut e);
    edit(&mut e, vec![Animation::Scale]);
    let (_, keys1, _, right1) = measure(&mut e);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&2].entrance_stack,
        vec![Animation::Scale],
        "stack kept"
    );
    assert!(right1 <= right0 + 0.5, "pane widened: {right0} {right1}");
    let grew = keys1.top() - keys0.top();
    let sub = |t: &str| {
        ctx.data(|d| {
            d.get_temp::<(egui::Rect, bool)>(egui::Id::unique(("property-subgroup-state", t)))
        })
    };
    assert!(
        (20. ..=40.).contains(&grew),
        "one effect row should add one row, added {grew}; keys {keys0:?} {keys1:?}; entrance {:?} exit {:?}",
        sub("Entrance"),
        sub("Exit")
    );
    edit(&mut e, vec![]);
    let (_, keys2, _, right2) = measure(&mut e);
    assert!((keys2.top() - keys0.top()).abs() < 0.5 && (right2 - right0).abs() < 0.5);
}
#[test]
fn video_magnify_shape_buttons_change_the_shape() {
    for auto_key in [false, true] {
        let ctx = egui::Context::default();
        let mut e = repair_tests::editor(&ctx);
        e.video.keyframes.auto_key = auto_key;
        let a = e
            .session
            .document
            .annotations
            .iter_mut()
            .find(|a| a.id == 2)
            .unwrap();
        a.kind = crate::Kind::Rectangle;
        a.text = None;
        a.fill = None;
        a.corner_radius = 0.;
        a.start = [300., 200.];
        a.end = [380., 260.];
        a.focus = Some(capture_core::focus::Focus {
            magnify: Some(Default::default()),
            ..Default::default()
        });
        e.select_only(Some(2));
        e.shell.properties = true;
        e.video.tab = PropertyTab::Style;
        e.video.object_tab = PropertyTab::Style;
        let find = |out: &crate::property_layout::tests::PaintedOutput, label: &str| {
            out.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()).center())
                }
                _ => None,
            })
        };
        for (label, check) in [
            (
                "Rounded",
                Box::new(|a: &crate::Annotation| {
                    a.kind == crate::Kind::Rectangle && a.corner_radius > 0.
                }) as Box<dyn Fn(&crate::Annotation) -> bool>,
            ),
            (
                "Circle",
                Box::new(|a: &crate::Annotation| a.kind == crate::Kind::Ellipse),
            ),
            (
                "Rectangle",
                Box::new(|a: &crate::Annotation| {
                    a.kind == crate::Kind::Rectangle && a.corner_radius == 0.
                }),
            ),
        ] {
            let pane = |e: &mut Editor, events: Vec<egui::Event>| {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(360., 2400.),
                        )),
                        events,
                        focused: true,
                        ..Default::default()
                    },
                    |ui| e.video_properties_pane(ui),
                );
                out.textures_delta.clear();
                crate::property_layout::tests::PaintedOutput(out)
            };
            pane(&mut e, vec![]);
            let out = pane(&mut e, vec![]);
            let at = find(&out, label).unwrap_or_else(|| {
                let texts: Vec<String> = out
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                        _ => None,
                    })
                    .collect();
                panic!("{label} button not drawn; texts: {texts:?}")
            });
            pane(&mut e, repair_tests::press(at, true));
            pane(&mut e, repair_tests::press(at, false));
            pane(&mut e, vec![]);
            let shown = e.selected_annotation().unwrap();
            assert!(
                check(&shown),
                "auto_key {auto_key}: {label} → {:?} r{}",
                shown.kind,
                shown.corner_radius
            );
        }
    }
}
#[test]
fn keyframe_controls_fit_compact_and_wide_panes() {
    for width in [290., 390.] {
        let ctx = egui::Context::default();
        let mut e = repair_tests::editor(&ctx);
        e.select_only(Some(2));
        e.video.tab = PropertyTab::Motion;
        e.video.object_tab = PropertyTab::Motion;
        e.video.keyframes.auto_key = true;
        e.video.keyframes.lanes = true;
        let t = e
            .session
            .document
            .video
            .as_mut()
            .unwrap()
            .objects
            .get_mut(&2)
            .unwrap();
        t.keyframes.insert(
            Property::Position,
            1500,
            Value::Numbers(vec![220., 100.]),
            Value::Numbers(vec![320., 180.]),
        );
        let mut draw = || {
            crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1100.),
                    )),
                    ..Default::default()
                },
                |ui| e.video_properties_pane(ui),
            ))
        };
        let mut first = draw();
        for _ in 0..3 {
            let out = draw();
            first.0.textures_delta.append(out.0.textures_delta.clone());
        }
        let last = draw();
        for shape in &last.shapes {
            if let egui::Shape::Text(t) = &shape.shape {
                assert!(
                    t.pos.x + t.galley.size().x <= width + 1.,
                    "Overflow: {}",
                    t.galley.text()
                );
            }
        }
        if let Some(folder) = std::env::var_os("CLIPCRAFT_GIF_EVIDENCE") {
            crate::property_layout::tests::dump_meshes(
                &ctx,
                &folder.to_string_lossy(),
                &format!("keyframes-{width}"),
                first,
                last,
            );
        }
    }
}

#[test]
fn step_markers_place_in_a_video_with_keyframes_on_or_off() {
    use egui::{Event, Modifiers, PointerButton};
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
    e.shell.backstage = None;
    e.shell.library = false;
    e.tool = Some(crate::Kind::Step);
    for _ in 0..3 {
        repair_tests::render(&ctx, &mut e, vec![]);
    }
    for (auto, pos) in [
        (true, egui::pos2(700., 220.)),
        (false, egui::pos2(380., 420.)),
    ] {
        e.video.keyframes.auto_key = auto;
        let before = e.session.document.annotations.len();
        let button = |pressed| Event::PointerButton {
            pos,
            pressed,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        };
        repair_tests::render(&ctx, &mut e, vec![Event::PointerMoved(pos), button(true)]);
        assert!(e.step_placement.is_some(), "keyframes {auto}");
        repair_tests::render(&ctx, &mut e, vec![button(false)]);
        assert_eq!(
            e.session.document.annotations.len(),
            before + 1,
            "keyframes {auto}: {}",
            e.status
        );
    }
}

#[test]
fn shapes_and_callouts_switch_both_ways_with_keyframes_on() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    let a = e
        .session
        .document
        .annotations
        .iter_mut()
        .find(|a| a.id == 1)
        .unwrap();
    a.kind = crate::Kind::Rectangle;
    a.text = None;
    a.step_outline = None;
    a.start = [100., 100.];
    a.end = [220., 180.];
    e.select_only(Some(1));
    e.video.keyframes.auto_key = true;
    for kind in [
        crate::Kind::Callout,
        crate::Kind::Rectangle,
        crate::Kind::Callout,
        crate::Kind::Ellipse,
    ] {
        let style = capture_core::object_styles::groups(crate::Kind::Rectangle)
            .into_iter()
            .flat_map(|g| g.styles)
            .find(|s| s.kind == kind)
            .unwrap();
        e.apply_object_template(&style, false);
        let now = e
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == 1)
            .unwrap()
            .kind;
        assert_eq!(now, kind, "{}", e.status);
    }
}
#[test]
fn a_trimmed_part_holds_its_last_pose_after_it_ends() {
    let ctx = egui::Context::default();
    let mut e = repair_tests::editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    let t = p.objects.get_mut(&2).unwrap();
    let start = t.start;
    t.keyframes = Default::default();
    t.keyframes.insert(
        Property::Position,
        2000,
        Value::Numbers(vec![100., 100.]),
        Value::Numbers(vec![500., 100.]),
    );
    t.end = start + 1000;
    p.cursor = start + 999;
    let base = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.id == 2)
        .unwrap()
        .clone();
    let at_end = e.keyframe_annotation(&base).body_bounds().0;
    e.session.document.video.as_mut().unwrap().cursor = start + 1800;
    let after = e.keyframe_annotation(&base).body_bounds().0;
    assert_eq!(
        at_end, after,
        "the selection frame must not keep moving after the part ends"
    );
}
