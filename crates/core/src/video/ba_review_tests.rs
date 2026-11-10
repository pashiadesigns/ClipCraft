//! BA review batch: keyframe color blending, step outline keys and zoom follow modes.
use super::keyframes::{Property, Set, Value};
use super::motion_path::{Node, Path, Purpose, Reduction};
use super::zoom::{Follow, FollowMode, Zoom};
use std::sync::Arc;

#[test]
fn fill_blends_from_none_and_text_colors_blend_between_discrete_keys() {
    let mut keys = Set::default();
    keys.insert(Property::Fill, 0, Value::Color(None), Value::Color(None));
    keys.insert(
        Property::Fill,
        1000,
        Value::Color(None),
        Value::Color(Some([200, 40, 40, 255])),
    );
    let Some(Value::Color(Some(mid))) = keys.sample(Property::Fill, 500) else {
        panic!("a fill fading in from none is a color mid-way");
    };
    assert_eq!(&mid[..3], &[200, 40, 40]);
    assert!(mid[3] > 0 && mid[3] < 255, "{mid:?}");
    assert_eq!(
        keys.sample(Property::Fill, 1000),
        Some(Value::Color(Some([200, 40, 40, 255])))
    );
    let text = |content: &str, color: [u8; 4], highlight: Option<[u8; 4]>| {
        let mut t = crate::text::TextStyle {
            content: content.into(),
            color: Some(color),
            ..Default::default()
        };
        let mut typography = t.typography();
        typography.highlight = highlight;
        t.set_typography(typography);
        Value::Text(Some(Box::new(t)))
    };
    let first = text("One", [0, 0, 0, 255], None);
    keys.insert(Property::Text, 0, first.clone(), first.clone());
    keys.insert(
        Property::Text,
        1000,
        first,
        text("Two", [255, 255, 255, 255], Some([255, 220, 0, 255])),
    );
    let Some(Value::Text(Some(mid))) = keys.sample(Property::Text, 500) else {
        panic!("text keys sample");
    };
    // Words stay discrete; colors and highlight move toward the next key.
    assert_eq!(mid.content, "One");
    let color = mid.color.unwrap();
    assert!(color[0] > 50 && color[0] < 205, "{color:?}");
    let highlight = mid.typography().highlight.unwrap();
    assert!(highlight[3] > 0 && highlight[3] < 255, "{highlight:?}");
}

#[test]
fn inline_editor_colors_blend_like_pane_colors_whatever_the_runs() {
    use crate::text::rich::{Format, RichText, Run};
    // Key 1: plain text in the object's color. Key 2: the inline editor colored
    // the second word red (a run that the first key doesn't have).
    let plain = crate::text::TextStyle {
        content: "Save now".into(),
        ..Default::default()
    };
    let mut colored = plain.clone();
    colored.rich = Some(RichText {
        runs: vec![Run {
            start: 5,
            end: 8,
            format: Format {
                color: Some([220, 30, 30, 255]),
                ..Format::from_style(&plain)
            },
        }],
        paragraphs: vec![],
    });
    let mut keys = Set::default();
    let first = Value::Text(Some(Box::new(plain.clone())));
    keys.insert(Property::Text, 0, first.clone(), first.clone());
    keys.insert(
        Property::Text,
        1000,
        first,
        Value::Text(Some(Box::new(colored))),
    );
    let ink = [255, 255, 255, 255];
    let Some(Value::Text(Some(mid))) = keys.sample_ink(Property::Text, 500, Some(ink)) else {
        panic!("text keys sample");
    };
    let word = crate::text::rich::format_at(&mid, 6).color.unwrap();
    assert!(
        word[1] > 30 && word[1] < 255,
        "the red word is mid-way: {word:?}"
    );
    let first_word = crate::text::rich::format_at(&mid, 1).color;
    assert!(first_word.is_none_or(|c| c == ink), "{first_word:?}");
    mid.rich.as_ref().unwrap().validate(&mid).unwrap();
}

#[test]
fn step_outline_color_and_width_are_keyed_separately_from_its_fill() {
    let mut a = crate::steps::marker(
        1,
        [50., 50.],
        40,
        [30, 90, 150, 255],
        (1, false),
        [400, 300],
    )
    .unwrap();
    a.step_outline = Some(crate::steps::Outline {
        color: [255, 255, 255, 255],
        width: 2.,
    });
    let mut keys = Set::default();
    let mut edited = a.clone();
    edited.step_outline = Some(crate::steps::Outline {
        color: [240, 20, 20, 255],
        width: 6.,
    });
    let base = keys.edit_annotation(&a, &edited, 1000, true);
    assert!(keys.channels.contains_key(&Property::Stroke));
    assert!(keys.channels.contains_key(&Property::StrokeWidth));
    assert!(!keys.channels.contains_key(&Property::Fill));
    let at_key = keys.annotation(&base, 1000);
    assert_eq!(at_key.step_outline.unwrap().color, [240, 20, 20, 255]);
    assert_eq!(at_key.color, a.color);
    let start = keys.annotation(&base, 0);
    assert_eq!(start.step_outline.unwrap().color, [255, 255, 255, 255]);
}

fn node(at: u64, x: f32) -> Node {
    Node {
        at,
        rect: [x, 100., 24., 24.],
        visible: true,
        sprite: 0,
        confidence: 1.,
    }
}

#[test]
fn zoom_follow_uses_the_shown_or_recorded_cursor_and_off_clears_the_track() {
    // Shown (reduced) cursor jumps straight to x=700; the recording wandered to x=100.
    let recorded: Vec<Node> = (0..=40).map(|i| node(i * 100, 100.)).collect();
    let path = Path {
        purpose: Purpose::Cursor,
        reference: [0., 0., 24., 24.],
        nodes: vec![node(0, 100.), node(500, 700.), node(4000, 700.)],
        sprites: vec![],
        smooth: false,
        enabled: true,
        offset_ms: 0,
        reviewed: true,
        reduction: Reduction::Clicks(600),
        inputs: vec![],
        recorded: Some(Arc::new(recorded)),
    };
    let zooms = |mode| {
        let mut a = Zoom::new(1, 0, 3000);
        a.entrance_ms = 200;
        a.exit_ms = 0;
        a.follow = Some(Follow {
            mode,
            tip: [0., 0.],
        });
        let mut b = Zoom::new(2, 3000, 4000);
        b.pan_from = Some(a.center);
        let mut zooms = vec![a, b];
        super::instruction::refollow(&mut zooms, &path, 0, [800, 600]);
        zooms
    };
    let shown = zooms(FollowMode::Shown);
    let recorded = zooms(FollowMode::Recorded);
    let off = zooms(FollowMode::Off);
    let end_x = |z: &[Zoom]| z[0].track.last().unwrap().1[0];
    assert!(end_x(&shown) > 0.8, "{}", end_x(&shown));
    assert!(end_x(&recorded) < 0.2, "{}", end_x(&recorded));
    assert_eq!(shown[1].pan_from, Some(shown[0].track.last().unwrap().1));
    assert!(off[0].track.is_empty());
    assert_eq!(off[1].pan_from, Some(off[0].center));
    for z in [&shown, &recorded, &off] {
        z[0].validate().unwrap();
    }
}

#[test]
fn zooms_share_a_track_unless_they_overlap() {
    use super::links::Layer;
    let mut p = super::editor::Project::new("t.mp4".into(), [640, 360], 20_000, 30);
    for (id, start, end) in [
        (1, 0, 2000),
        (2, 2000, 4000),
        (3, 5000, 7000),
        (4, 6000, 8000),
    ] {
        p.zooms.push(Zoom::new(id, start, end));
    }
    p.pack_zooms(&[1, 2, 3, 4]);
    let tracks: Vec<u64> = p.zooms.iter().map(|z| z.track()).collect();
    assert_eq!(tracks[0], tracks[1]);
    assert_eq!(tracks[0], tracks[2]);
    assert_ne!(
        tracks[2], tracks[3],
        "overlapping zooms need a second track"
    );
    let zoom_layers = p
        .layers()
        .into_iter()
        .filter(|l| matches!(l, Layer::Zoom(_)))
        .count();
    assert_eq!(zoom_layers, 2);
    // Deleting a track removes only the zooms on it.
    p.remove_layer(Layer::Zoom(tracks[3]));
    assert_eq!(p.zooms.len(), 3);
}

#[test]
fn step_outlines_take_shape_outline_styles() {
    let mut a = crate::steps::marker(
        1,
        [60., 60.],
        60,
        [30, 90, 150, 255],
        (1, false),
        [200, 200],
    )
    .unwrap();
    a.step_outline = Some(crate::steps::Outline {
        color: [250, 250, 250, 255],
        width: 6.,
    });
    let render = |a: &crate::Annotation| {
        let mut d = crate::Document::canvas(200, 200, [0, 0, 0, 255]).unwrap();
        d.annotations.push(a.clone());
        d.validate().unwrap();
        d.render().unwrap()
    };
    let solid = render(&a);
    a.line_style = Some(crate::line_style::Style {
        pattern: crate::line_style::Pattern::Dashed,
        ..crate::line_style::Style::for_kind(crate::Kind::Step)
    });
    assert_ne!(render(&a), solid, "a dashed step outline draws differently");
}

#[test]
fn text_size_blends_between_keys() {
    let small = crate::text::TextStyle {
        content: "Grow".into(),
        size_points: 12.,
        ..Default::default()
    };
    let big = crate::text::TextStyle {
        size_points: 36.,
        ..small.clone()
    };
    let mut keys = Set::default();
    let first = Value::Text(Some(Box::new(small)));
    keys.insert(Property::Text, 0, first.clone(), first.clone());
    keys.insert(
        Property::Text,
        1000,
        first,
        Value::Text(Some(Box::new(big))),
    );
    let Some(Value::Text(Some(mid))) = keys.sample(Property::Text, 500) else {
        panic!("text keys sample");
    };
    assert!(
        mid.size_points > 12. && mid.size_points < 36.,
        "{}",
        mid.size_points
    );
    mid.validate().unwrap();
}

#[test]
fn a_timing_edit_reaches_other_objects_without_animations_they_cannot_use() {
    use super::editor::{Animation, Timing};
    let pixels = std::sync::Arc::new(image::RgbaImage::new(4, 4));
    let mut blur =
        std::sync::Arc::new(crate::bitmap::Bitmap::transient(pixels).unwrap()).annotation(1);
    blur.kind = crate::Kind::Blur;
    let before = Timing::new(1, 1000, 3000);
    let mut after = before.clone();
    after.entrance = Animation::Fade;
    after.entrance_ms = 200;
    after.start = 1500;
    let mut other = Timing::new(2, 5000, 6000);
    other.copy_change(&before, &after, &blur);
    assert_eq!(
        other.entrance,
        Animation::None,
        "privacy objects never fade"
    );
    assert_eq!(other.entrance_ms, 200);
    assert_eq!((other.start, other.end), (5500, 6000));
}
#[test]
fn direct_paths_keep_a_recording_that_has_no_clicks() {
    use super::motion_path::{Node, Reduction, reduce};
    let nodes: Vec<Node> = (0..20)
        .map(|n| Node {
            at: n * 50,
            rect: [n as f32 * 9., 0., 24., 24.],
            visible: true,
            sprite: 0,
            confidence: 1.,
        })
        .collect();
    assert_eq!(reduce(&nodes, &[], Reduction::Clicks(300)), nodes);
}
#[test]
fn a_keyframe_copy_leaves_the_original() {
    use super::keyframes::{Property, Set, Value};
    let mut set = Set::default();
    set.insert(
        Property::Opacity,
        100,
        Value::Numbers(vec![1.]),
        Value::Numbers(vec![0.5]),
    );
    set.copy_key(100, 400, Some(Property::Opacity)).unwrap();
    assert_eq!(
        set.sample(Property::Opacity, 100),
        Some(Value::Numbers(vec![0.5]))
    );
    assert_eq!(
        set.sample(Property::Opacity, 400),
        Some(Value::Numbers(vec![0.5]))
    );
    assert!(set.copy_key(100, 400, Some(Property::Opacity)).is_err());
}
