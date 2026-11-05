use crate::video::{
    editor::{Animation, Project, Timing},
    links::Layer,
};
use crate::*;
fn shape(id: u64, kind: Kind) -> Annotation {
    let mut a = crate::steps::marker(
        id,
        [60., 45.],
        48,
        [25, 154, 195, 255],
        (1, false),
        [160, 100],
    )
    .unwrap();
    a.kind = kind;
    a.start = [20., 20.];
    a.end = [120., 70.];
    a.text = None;
    a.fill = Some([30, 180, 100, 255]);
    a.width = 3;
    a.step_outline = None;
    a
}
#[test]
fn new_annotation_tracks_are_above_existing_content_and_future_placement_extends_export() {
    let mut d = Document::blank(160, 100).unwrap();
    let mut p = Project::new("recording.mp4".into(), [160, 100], 2000, 30);
    p.layer_order = vec![Layer::Media(1), Layer::Annotation(1)];
    p.place(1, Timing::new(1, 0, 1000));
    p.place(2, Timing::new(1, 0, 1000));
    let track = p.objects[&2].track;
    assert_eq!(p.layers()[0], Layer::Annotation(track));
    p.cursor = 9000;
    d.annotations = vec![
        shape(1, Kind::Rectangle),
        shape(2, Kind::Ellipse),
        shape(3, Kind::Rectangle),
    ];
    p.sync(&d.annotations);
    assert_eq!(p.objects[&3].start, 9000);
    assert_eq!(p.duration(), 13000);
    d.video = Some(p);
    d.validate().unwrap();
}
#[test]
fn animation_entries_keep_independent_durations_and_retraction_endpoints() {
    let a = shape(1, Kind::Rectangle);
    let mut t = Timing::new(1, 0, 4000);
    t.entrance = Animation::Fade;
    t.entrance_ms = 200;
    t.entrance_stack = vec![Animation::Path];
    t.entrance_durations.insert(Animation::Path, 1200);
    t.smooth = false;
    t.clamp_animations();
    let parts = crate::video::editor::animated_parts(&a, &t, 600);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].color[3], 255);
    assert!(parts[1].points.len() > 1);
    let saved = crate::reusable_presets::Motion::from(&t);
    let mut applied = Timing::new(3, 5000, 9000);
    saved.apply(&a, &mut applied).unwrap();
    assert_eq!(applied.entrance_durations, t.entrance_durations);
    let mut line = shape(2, Kind::Line);
    line.fill = None;
    line.line_style = Some(crate::line_style::Style::for_kind(Kind::Line));
    t = Timing::new(1, 0, 1000);
    t.entrance_ms = 0;
    t.exit = Animation::Path;
    t.exit_ms = 500;
    t.smooth = false;
    let from_last = crate::video::editor::animated(&line, &t, 750).unwrap();
    t.retract_from_start = true;
    let from_first = crate::video::editor::animated(&line, &t, 750).unwrap();
    use vello_cpu::kurbo::ParamCurve;
    let first = crate::line_style::geometry(&from_last)
        .shaft
        .segments()
        .next()
        .unwrap()
        .start();
    let second = crate::line_style::geometry(&from_first)
        .shaft
        .segments()
        .next()
        .unwrap()
        .start();
    assert!(second.x > first.x + 20.);
}
#[test]
fn captured_activity_captions_remain_editable_guide_text() {
    use crate::step_capture::{Button, Gesture, Guide};
    let mut guide = Guide::new([80, 60]).unwrap();
    for instruction in [
        "Scroll down to the view shown",
        "Type \"Sample content\" in the \"Search\" box",
        "Press Enter",
    ] {
        guide
            .push(
                image::RgbaImage::from_pixel(80, 60, image::Rgba([255; 4])),
                Gesture {
                    start: [20, 20],
                    end: [20, 20],
                    button: Button::Left,
                },
            )
            .unwrap();
        assert!(guide.set_instruction(guide.len() - 1, instruction));
    }
    let d = guide.document().unwrap();
    let labels: Vec<_> = d
        .annotations
        .iter()
        .filter_map(|a| a.text.as_ref())
        .map(|t| t.content.as_str())
        .collect();
    assert!(labels.contains(&"Press Enter"));
    assert!(labels.iter().any(|s| s.contains("Sample content")));
    assert!(guide.undo_step());
    assert!(
        !guide
            .document()
            .unwrap()
            .annotations
            .iter()
            .any(|a| a.text.as_ref().is_some_and(|t| t.content == "Press Enter"))
    );
}

#[test]
fn empty_automatic_tracks_are_removed_but_manual_tracks_survive() {
    use crate::video::editor::{Project, Timing};
    let mut p = Project::new("C:/video.mp4".into(), [160, 90], 2000, 30);
    let manual = p.add_track();
    for id in 1..=3 {
        p.place(id, Timing::new(0, 0, 1000));
    }
    let automatic = p.objects[&3].track;
    assert!(p.automatic_tracks.contains(&automatic));
    p.objects.remove(&3);
    p.prune_automatic_tracks();
    assert!(!p.tracks.contains(&automatic));
    p.objects.clear();
    p.prune_automatic_tracks();
    assert_eq!(p.tracks, vec![manual]);
    let roundtrip: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(roundtrip, p);
}
#[test]
fn compatible_preset_update_is_atomic_and_retains_identity() {
    use crate::reusable_presets::{Motion, Payload, Store};
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let mut motion = Motion::from(&Timing::new(1, 0, 2000));
    let id = store
        .save("First", Payload::Motion(motion.clone()))
        .unwrap();
    motion.entrance = Animation::Fade;
    motion.entrance_ms = 750;
    store
        .update(id, "Updated", Payload::Motion(motion.clone()))
        .unwrap();
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.entries()[0].id, id);
    assert_eq!(store.entries()[0].payload, Payload::Motion(motion));
    let before = store.entries().to_vec();
    assert!(
        store
            .update(id, "", Payload::AudioChain(Default::default()))
            .is_err()
    );
    assert_eq!(store.entries(), before);
}
#[test]
fn cached_export_matches_dynamic_compositor_and_invalidates_edits() {
    let mut d = Document::blank(160, 100).unwrap();
    let mut p = Project::new("screen.mp4".into(), [160, 100], 2000, 30);
    let mut a = shape(1, Kind::Rectangle);
    a.shadow = Some(Default::default());
    a.fill = Some([80, 190, 120, 180]);
    d.annotations.push(a);
    p.sync(&d.annotations);
    d.video = Some(p);
    let mut cache = crate::video::frame::CompositeCache::default();
    for n in 0..3 {
        if n == 2 {
            d.annotations[0].start[0] += 10.;
        }
        let expected = crate::video::frame::composite(&d, &[], 1000).unwrap();
        let cached = cache.composite(&d, &[], 1000).unwrap();
        assert!(
            expected
                .as_raw()
                .iter()
                .zip(cached.as_raw())
                .all(|(a, b)| a.abs_diff(*b) <= 2)
        );
    }
    let mut redact = shape(2, Kind::Redact);
    redact.fill = None;
    d.annotations.push(redact);
    d.video.as_mut().unwrap().sync(&d.annotations);
    assert_eq!(
        cache.composite(&d, &[], 1000).unwrap().get_pixel(60, 45),
        &image::Rgba([0, 0, 0, 255])
    );
}
