use super::{
    editor::*,
    effects::{Chain, Effect, Kind as Fx, Processor},
    links::Layer,
    zoom::{Motion, Zoom},
};
use crate::{Command, Document, Session};
fn document() -> Document {
    let mut d = Document::blank(320, 180).unwrap();
    d.video = Some(Project::new("recording.mp4".into(), [320, 180], 1000, 30));
    d
}
#[test]
fn zoom_inverse_edit_mapping_and_ripple_keep_ranges_valid() {
    let mut p = document().video.unwrap();
    let mut first = Zoom::new(1, 0, 5000);
    first.entrance_ms = 0;
    first.exit_ms = 0;
    first.center = [0.4, 0.6];
    let mut second = first.clone();
    second.id = 2;
    second.scale = 3.;
    p.zooms = vec![first, second];
    let original = [80., 100.];
    let view = p.layer_view(Layer::Media(1), [320, 180], 1000);
    let point = view.inverse(view.point(original));
    assert!((point[0] - original[0]).abs() < 0.001 && (point[1] - original[1]).abs() < 0.001);
    assert!((view.scale - 6.).abs() < 0.0001);
    p.remove_range(1000, 2000).unwrap();
    assert_eq!(p.zooms[0].end, 4000);
    p.validate(&document()).unwrap();
    let mut removed = p.clone();
    removed.zooms[1].start = 1000;
    removed.zooms[1].end = 2000;
    removed.zooms[1].entrance_ms = 0;
    removed.zooms[1].exit_ms = 0;
    removed.layer_order = vec![Layer::Zoom(2), Layer::Zoom(1), Layer::Media(1)];
    removed.remove_range(1000, 2000).unwrap();
    assert_eq!(removed.zooms.len(), 1);
    assert!(!removed.layer_order.contains(&Layer::Zoom(2)));
    removed.validate(&document()).unwrap();
    p.zooms[0].scale = 10.;
    p.zooms[1].scale = 10.;
    assert!(p.validate(&document()).is_err());
}
#[test]
fn view_only_gestures_do_not_make_undo_and_icon_paste_is_a_video_edit() {
    let mut s = Session::new(document()).unwrap();
    s.begin_gesture();
    let p = s.document.video.as_mut().unwrap();
    p.cursor = 700;
    p.annotation_track = Some(1);
    s.finish_gesture();
    assert!(!s.can_undo());
    let image = image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 120, 180, 255]));
    let b = crate::bitmap::Bitmap::transient(std::sync::Arc::new(image))
        .unwrap()
        .into_icon();
    s.begin_gesture();
    s.execute(Command::Add {
        annotation: std::sync::Arc::new(b).annotation(1),
    })
    .unwrap();
    s.finish_gesture();
    assert_eq!(s.document.video.as_ref().unwrap().objects[&1].start, 700);
    s.execute(Command::Undo).unwrap();
    assert!(s.document.annotations.is_empty());
    assert!(!s.can_undo());
}
#[test]
fn active_track_is_only_an_insertion_preference_never_an_implicit_move() {
    let mut p = document().video.unwrap();
    let second = p.add_track();
    let third = p.add_track();
    p.place(1, Timing::new(1, 0, 500));
    p.annotation_track = Some(third);
    p.place(1, p.objects[&1].clone());
    assert_eq!(p.objects[&1].track, 1);
    let mut moved = p.objects[&1].clone();
    moved.track = second;
    p.place(1, moved);
    assert_eq!(p.objects[&1].track, second);
    p.place(2, Timing::new(0, 500, 1000));
    assert_eq!(p.objects[&2].track, third);
}
#[test]
fn native_extended_local_narration_paths_and_takes_extend_project_duration() {
    assert!(local_media_path(r"\\?\C:\Clips\take.wav"));
    assert!(!local_media_path(r"\\server\share\take.wav"));
    assert!(!local_media_path(r"\\?\UNC\server\take.wav"));
    assert!(!local_media_path(r"\\.\device"));
    let mut d = document();
    let p = d.video.as_mut().unwrap();
    let track = p.add_typed_track(TrackKind::Voiceover);
    let source = Source {
        path: r"\\?\C:\Clips\take.wav".into(),
        size: [1, 1],
        duration: 3000,
        kind: MediaKind::Audio,
    };
    p.segments
        .push(Segment::new(2, 5000, track, Some(source), 3000));
    assert_eq!(p.duration(), 8000);
    d.validate().unwrap();
}
#[test]
fn video_steps_commit_without_expanding_canvas_and_have_timing() {
    let mut s = Session::new(document()).unwrap();
    let before = s.document.clone();
    let a =
        crate::steps::marker(1, [90., 80.], 44, [0, 0, 0, 255], (1, false), [320, 180]).unwrap();
    s.execute(Command::Add { annotation: a }).unwrap();
    assert_eq!(s.document.annotations.len(), 1);
    assert!(s.document.video.as_ref().unwrap().objects.contains_key(&1));
    assert_eq!([s.document.width, s.document.height], [320, 180]);
    s.execute(Command::Undo).unwrap();
    assert_eq!(
        serde_json::to_value(&s.document).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
#[test]
fn live_adjustments_are_one_undo_and_cancel_preserves_redo() {
    let mut s = Session::new(document()).unwrap();
    let before = s.document.clone();
    s.begin_gesture();
    for i in 1..=80 {
        let mut p = s.document.video.clone().unwrap();
        p.segments[0].rect = [0., 0., 0.2 + i as f32 / 100., 0.5];
        s.execute(Command::Video { project: p }).unwrap();
    }
    let after = s.document.clone();
    s.finish_gesture();
    s.execute(Command::Undo).unwrap();
    assert_eq!(
        serde_json::to_value(&s.document).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    assert!(!s.can_undo());
    s.begin_gesture();
    let mut p = s.document.video.clone().unwrap();
    p.fill = true;
    s.execute(Command::Video { project: p }).unwrap();
    s.cancel_edit_gesture();
    assert_eq!(
        serde_json::to_value(&s.document).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    assert!(s.can_redo());
    s.execute(Command::Redo).unwrap();
    assert_eq!(
        serde_json::to_value(&s.document).unwrap(),
        serde_json::to_value(&after).unwrap()
    );
}
#[test]
fn track_delete_restores_entire_project_with_one_undo() {
    let mut s = Session::new(document()).unwrap();
    let before = s.document.clone();
    s.execute(Command::VideoDeleteTrack {
        layer: Layer::Media(1),
    })
    .unwrap();
    assert!(s.document.video.as_ref().unwrap().segments.is_empty());
    s.execute(Command::Undo).unwrap();
    assert_eq!(
        serde_json::to_value(&s.document).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
#[test]
fn zoom_preserves_sources_and_transforms_only_lower_layers_with_pin_behavior() {
    let mut d = document();
    let mut a =
        crate::steps::marker(1, [80., 80.], 24, [240, 0, 0, 255], (1, false), [320, 180]).unwrap();
    a.text = None;
    d.annotations.push(a.clone());
    let p = d.video.as_mut().unwrap();
    p.sync(&d.annotations);
    let mut z = Zoom::new(1, 0, 1000);
    z.entrance = Motion::Snap;
    z.entrance_ms = 0;
    z.exit_ms = 0;
    z.center = [0.25, 0.5];
    z.pinned = false;
    p.zooms.push(z);
    p.layer_order = vec![Layer::Annotation(1), Layer::Zoom(1), Layer::Media(1)];
    let out = super::frame::ordered_objects(&d, &[], 500, false);
    assert_eq!(out[0].start, a.start);
    let p = d.video.as_mut().unwrap();
    p.reorder_layer(Layer::Zoom(1), 0);
    assert!(p.zooms[0].pinned);
    p.add_track();
    assert_eq!(p.layers()[0], Layer::Zoom(1));
    let out = super::frame::ordered_objects(&d, &[], 500, false);
    assert_ne!(out[0].start, a.start);
    assert_eq!(d.annotations[0], a);
    let copy: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&copy).unwrap(),
        serde_json::to_value(&d).unwrap()
    );
    let p = d.video.as_mut().unwrap();
    p.reorder_layer(Layer::Zoom(1), 2);
    assert!(!p.zooms[0].pinned);
    assert_eq!(p.layers()[2], Layer::Zoom(1));
}
#[test]
fn zoom_transitions_are_continuous_bounded_and_aspect_locked() {
    let z = Zoom::new(1, 0, 2000);
    assert_eq!(z.view(0), Some([0., 0., 1.]));
    assert_eq!(z.view(1000), Some([0.25, 0.25, 0.5]));
    assert!(z.view(1999).unwrap()[2] > 0.999);
    assert_eq!(z.view(2000), None);
    for t in 0..2000 {
        let v = z.view(t).unwrap();
        assert!(v[0] >= 0. && v[1] >= 0. && v[0] + v[2] <= 1.001 && v[1] + v[2] <= 1.001);
    }
}
#[test]
fn original_processed_mix_is_exact_and_history_survives_effect_switches() {
    let mut effect = Effect::new(1, Fx::Equalizer);
    effect.values[0] = 6.;
    let full = Chain {
        mix: 1.,
        bypass: false,
        effects: vec![effect],
    };
    let mut half = full.clone();
    half.mix = 0.5;
    let mut dry = full.clone();
    dry.mix = 0.;
    let input = vec![[0.03, -0.04]; 1000];
    let mut wet = input.clone();
    let mut middle = input.clone();
    let mut original = input.clone();
    Processor::new(&full).unwrap().process(&mut wet);
    Processor::new(&half).unwrap().process(&mut middle);
    Processor::new(&dry).unwrap().process(&mut original);
    assert_eq!(original, input);
    for i in 0..1000 {
        assert!((middle[i][0] - (input[i][0] + wet[i][0]) * 0.5).abs() < 1e-9);
    }
    let chain = Chain::narration();
    let mut dsp = Processor::new(&chain).unwrap();
    dsp.set_visualization(Some(1));
    dsp.process(&mut vec![[0.1, 0.1]; 4800]);
    for id in 1..=6 {
        dsp.set_visualization(Some(id));
        dsp.process(&mut [[0.1, 0.1]; 960]);
        assert!(
            dsp.meters
                .iter()
                .all(|m| m.history.is_some_and(|h| h.len >= 5))
        );
    }
    assert!(dsp.meters[0].spectrum.is_some());
    assert!(dsp.meters[2].spectrum.is_some());
}
