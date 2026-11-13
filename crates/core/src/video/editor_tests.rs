use super::editor::*;
use crate::{Command, Document, Kind, Session};
fn doc() -> Document {
    let mut d = Document::blank(320, 180).unwrap();
    d.video = Some(Project::new("source.mp4".into(), [320, 180], 10000, 30));
    d.canvas_behavior.dynamic_expansion = false;
    d
}
fn marker(id: u64) -> crate::Annotation {
    crate::steps::marker(
        id,
        [80., 80.],
        44,
        [200, 10, 20, 255],
        (id as u32, false),
        [320, 180],
    )
    .unwrap()
}
#[test]
fn tracks_reuse_full_free_ranges_and_preserve_other_assignments() {
    let mut v = doc().video.unwrap();
    v.place(1, Timing::new(0, 0, 1000));
    v.place(2, Timing::new(0, 1000, 2000));
    assert_eq!(v.objects[&1].track, v.objects[&2].track);
    v.place(3, Timing::new(0, 500, 1500));
    assert_eq!(v.tracks.len(), 2);
    let old = v.objects[&2].clone();
    v.place(1, Timing::new(v.objects[&1].track, 1400, 1800));
    assert_eq!(v.objects[&2], old);
    assert_eq!(v.tracks.len(), 3);
}
#[test]
fn cuts_retime_objects_and_undo_restores_every_segment() {
    let mut d = doc();
    d.annotations = vec![marker(1), marker(2), marker(3)];
    let v = d.video.as_mut().unwrap();
    v.split(3000).unwrap();
    v.split(7000).unwrap();
    v.place(1, Timing::new(0, 1000, 4000));
    v.place(2, Timing::new(0, 4000, 5000));
    v.place(3, Timing::new(0, 6000, 9000));
    let mut s = Session::new(d.clone()).unwrap();
    s.execute(Command::VideoRemove {
        start: 2500,
        end: 5500,
    })
    .unwrap();
    let v = s.document.video.as_ref().unwrap();
    assert_eq!(v.duration(), 7000);
    assert_eq!(v.objects[&1].end, 2500);
    assert!(!v.objects.contains_key(&2));
    assert_eq!((v.objects[&3].start, v.objects[&3].end), (3000, 6000));
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.video, d.video);
    assert_eq!(s.document.annotations, d.annotations);
    s.execute(Command::Redo).unwrap();
    assert_eq!(s.document.video.as_ref().unwrap().duration(), 7000);
}
#[test]
fn speed_retimes_ranges_without_losing_objects() {
    let mut v = doc().video.unwrap();
    v.split(4000).unwrap();
    v.place(1, Timing::new(0, 1000, 3000));
    v.place(2, Timing::new(0, 5000, 6000));
    v.speed(0, 2.).unwrap();
    assert_eq!(v.duration(), 8000);
    assert_eq!((v.objects[&1].start, v.objects[&1].end), (500, 1500));
    assert_eq!(v.objects[&2].start, 3000);
    assert_eq!(v.source_time(3000), 5000);
}
#[test]
fn redaction_never_fades_and_does_not_expose_source_pixels() {
    let mut d = doc();
    let mut a = marker(1);
    a.kind = Kind::Redact;
    a.start = [20., 20.];
    a.end = [120., 120.];
    a.text = None;
    a.color = [0, 0, 0, 255];
    d.annotations.push(a.clone());
    let v = d.video.as_mut().unwrap();
    v.sync(&d.annotations);
    let t = v.objects.get_mut(&1).unwrap();
    t.entrance = Animation::Fade;
    assert!(d.validate().is_err());
    let mut t = Timing::new(1, 0, 2000);
    t.entrance = Animation::Fade;
    assert_eq!(animated(&a, &t, 0), Some(a));
    d.video.as_mut().unwrap().sync(&d.annotations);
    let frame = image::RgbaImage::from_pixel(320, 180, image::Rgba([255, 255, 255, 255]));
    let out = composite(&d, &frame, 0).unwrap();
    assert_eq!(out.get_pixel(50, 50).0, [0, 0, 0, 255]);
    assert_eq!(frame.get_pixel(50, 50).0, [255, 255, 255, 255]);
}
#[test]
fn typewriter_keeps_emoji_graphemes_and_cursor_is_not_an_edit() {
    let mut a = marker(1);
    a.text.as_mut().unwrap().content = "A👩🏽‍💻B".into();
    let mut t = Timing::new(1, 0, 2000);
    t.entrance = Animation::Typewriter;
    t.entrance_ms = 900;
    t.smooth = false;
    let a = animated(&a, &t, 400).unwrap();
    assert_eq!(a.text.unwrap().content, "A👩🏽‍💻");
    let mut d = doc();
    let old = d.video.clone().unwrap();
    d.video.as_mut().unwrap().cursor = 600;
    assert!(d.video.as_ref().unwrap().same_edit(&old));
}
#[test]
fn portable_project_copies_media_once_and_reopens_from_another_folder() {
    let root = tempfile::tempdir().unwrap();
    let media = root.path().join("original.mp4");
    std::fs::write(&media, b"synthetic local media").unwrap();
    let folder = root.path().join("saved");
    std::fs::create_dir(&folder).unwrap();
    let path = folder.join("Guide.vclip");
    let mut d = doc();
    let v = d.video.as_mut().unwrap();
    v.source = media.to_string_lossy().into_owned();
    v.segments[0].audio.push(Audio {
        path: v.source.clone(),
        label: "Recording".into(),
        volume: 1.,
        muted: false,
        fade_in: 0,
        fade_out: 0,
    });
    d.save(&path).unwrap();
    let count = || {
        std::fs::read_dir(path.with_extension("vclip-media"))
            .unwrap()
            .count()
    };
    assert_eq!(count(), 1);
    let loaded = Document::load(&path).unwrap();
    loaded.save(&path).unwrap();
    assert_eq!(count(), 1);
    assert!(Path::new(&loaded.video.as_ref().unwrap().source).is_file());
    let raw = std::fs::read_to_string(path).unwrap();
    assert!(!raw.contains("original.mp4"));
}
use std::path::Path;

#[test]
fn media_overlap_fades_tracks_and_ripple_keep_source_intervals() {
    let mut d = doc();
    let v = d.video.as_mut().unwrap();
    v.segments[0].end = 4000;
    let mut b = v.segments[0].clone();
    b.id = 2;
    b.at = 3000;
    b.start = 1000;
    b.end = 5000;
    v.segments.push(b);
    assert!((v.opacity(&v.segments[0], 3500) - 0.5).abs() < 0.001);
    assert!((v.opacity(&v.segments[1], 3500) - 0.5).abs() < 0.001);
    assert_eq!(v.duration(), 7000);
    v.segments[1].fade_out = 500;
    assert!((v.opacity(&v.segments[1], 6750) - 0.5).abs() < 0.001);
    let old = d.clone();
    let mut s = Session::new(d).unwrap();
    s.execute(Command::VideoRemove {
        start: 1500,
        end: 2500,
    })
    .unwrap();
    let v = s.document.video.as_ref().unwrap();
    assert_eq!(v.duration(), 6000);
    assert!(v.segments.iter().any(|c| c.at == 1500 && c.start == 2500));
    assert!(v.segments.iter().any(|c| c.at == 2000 && c.start == 1000));
    s.execute(Command::Undo).unwrap();
    assert_eq!(
        serde_json::to_vec(&s.document).unwrap(),
        serde_json::to_vec(&old).unwrap()
    );
}

#[test]
fn invalid_overlap_and_remote_media_rejected_before_editing() {
    let mut d = doc();
    let v = d.video.as_mut().unwrap();
    for id in 2..=3 {
        let mut c = v.segments[0].clone();
        c.id = id;
        c.at = id * 100;
        v.segments.push(c);
    }
    assert!(
        d.validate()
            .unwrap_err()
            .to_string()
            .contains("Three clips overlap")
    );
    let v = d.video.as_mut().unwrap();
    v.segments.pop();
    v.segments[1].source = Some(Source {
        path: "https://example.com/track.mp4".into(),
        duration: 10000,
        size: [320, 180],
        kind: MediaKind::Video,
    });
    assert!(d.validate().is_err());
    let v = d.video.as_mut().unwrap();
    v.segments[1].source.as_mut().unwrap().path = "music.wav".into();
    v.segments[1].source.as_mut().unwrap().kind = MediaKind::Audio;
    assert!(
        d.validate()
            .unwrap_err()
            .to_string()
            .contains("need their own tracks")
    );
    let v = d.video.as_mut().unwrap();
    v.segments[1].track = v.add_media_track();
    assert!(d.validate().is_ok());
}

#[test]
fn draw_animation_follows_the_original_curved_path() {
    use vello_cpu::kurbo::ParamCurveArclen;
    let mut a = marker(1);
    a.kind = Kind::Arrow;
    a.text = None;
    a.start = [20., 130.];
    a.end = [280., 30.];
    let mut style = crate::line_style::Style::for_kind(Kind::Arrow);
    style.curve = crate::line_style::Curve::Bezier;
    style.bend = 0.4;
    a.line_style = Some(style);
    let full = crate::line_style::geometry(&a).full_shaft;
    let mut t = Timing::new(1, 0, 2000);
    t.entrance = Animation::Path;
    t.entrance_ms = 1000;
    t.smooth = false;
    let sampled = animated(&a, &t, 500).unwrap();
    let partial = crate::line_style::geometry(&sampled).full_shaft;
    let length = |p: &vello_cpu::kurbo::BezPath| p.segments().map(|s| s.arclen(1e-3)).sum::<f64>();
    assert!((length(&partial) / length(&full) - 0.5).abs() < 0.001);
    assert_eq!(sampled.start, a.start);
    assert_eq!(sampled.end, a.end);
    assert!(a.line_style.unwrap().reveal.is_none());
}
#[test]
fn drawing_on_a_shape_keeps_its_outline_pattern_and_shadow_while_animating() {
    let mut a = marker(1);
    a.kind = Kind::Rectangle;
    a.text = None;
    a.fill = None;
    a.width = 6;
    a.start = [30., 30.];
    a.end = [230., 130.];
    let mut style = crate::line_style::Style::for_kind(Kind::Rectangle);
    style.pattern = crate::line_style::Pattern::Dashed;
    a.line_style = Some(style);
    a.shadow = Some(crate::shadow::Shadow::default());
    let mut t = Timing::new(1, 0, 2000);
    t.entrance = Animation::Path;
    t.entrance_ms = 1000;
    t.exit = Animation::Path;
    t.exit_ms = 1000;
    t.smooth = false;
    let render = |a: &crate::Annotation| {
        let mut image = image::RgbaImage::new(260, 160);
        crate::display::render_vector(
            &mut image,
            a,
            crate::display::Viewport {
                size: [260, 160],
                origin: [0., 0.],
                scale: 1.,
            },
        );
        image
    };
    for time in [500, 1500] {
        let sampled = animated(&a, &t, time).unwrap();
        assert_eq!(sampled.kind, Kind::Pen, "at {time}");
        assert_eq!(
            sampled.line_style.map(|s| s.pattern),
            Some(crate::line_style::Pattern::Dashed)
        );
        assert_eq!(sampled.shadow, a.shadow);
        assert!(crate::shadow::patch(&sampled).is_some(), "shadow at {time}");
        let mut solid = sampled.clone();
        solid.line_style = None;
        let ink = |i: &image::RgbaImage| i.pixels().filter(|p| p[3] > 0).count();
        let (dashed, solid) = (ink(&render(&sampled)), ink(&render(&solid)));
        assert!(
            dashed > 0 && dashed * 10 < solid * 9,
            "dashed {dashed} vs solid {solid}"
        );
    }
}
#[test]
fn portable_media_metadata_collision_keeps_both_contents() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a.mp4");
    let b = root.path().join("b.mp4");
    std::fs::write(&a, b"aaaa").unwrap();
    std::fs::write(&b, b"bbbb").unwrap();
    let times = std::fs::FileTimes::new().set_modified(a.metadata().unwrap().modified().unwrap());
    std::fs::File::options()
        .write(true)
        .open(&b)
        .unwrap()
        .set_times(times)
        .unwrap();
    let output = root.path().join("saved.vclip");
    let mut d = doc();
    d.video.as_mut().unwrap().source = a.to_string_lossy().into_owned();
    d.save(&output).unwrap();
    let old = Document::load(&output).unwrap();
    d.video.as_mut().unwrap().source = b.to_string_lossy().into_owned();
    d.save(&output).unwrap();
    let new = Document::load(&output).unwrap();
    assert_eq!(std::fs::read(old.video.unwrap().source).unwrap(), b"aaaa");
    assert_eq!(std::fs::read(new.video.unwrap().source).unwrap(), b"bbbb");
}
