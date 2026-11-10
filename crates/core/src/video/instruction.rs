//! Instruction recording produces ordinary editable timeline annotations/Zooms.
use super::{
    editor::{Animation, Timing},
    links::Layer,
    motion_path::{Input, Path, Reduction},
    zoom::Zoom,
};
use crate::{Annotation, Document, Kind, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Clicks closer together than this keep the zoom in and pan to the next target.
pub const DEFAULT_ZOOM_COOLDOWN_MS: u64 = 3000;
pub const MAX_ZOOM_COOLDOWN_MS: u64 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub enabled: bool,
    pub cursor_reduction: Reduction,
    pub click_highlights: bool,
    pub zoom_clicks: bool,
    pub zoom_cooldown_ms: u64,
    /// While zoomed between nearby clicks, pan with the moving cursor.
    pub zoom_follow_cursor: bool,
    pub captions: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: false,
            cursor_reduction: Reduction::Clicks(Reduction::DEFAULT_LEAD),
            click_highlights: true,
            zoom_clicks: true,
            zoom_cooldown_ms: DEFAULT_ZOOM_COOLDOWN_MS,
            zoom_follow_cursor: true,
            captions: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub at: u64,
    pub point: [f32; 2],
    pub button: u8,
    pub dragging: bool,
    #[serde(default)]
    pub release: Option<u64>,
    pub caption: String,
}
fn shape(id: u64, kind: Kind, rect: [f32; 4], size: [u32; 2]) -> Annotation {
    let mut a =
        crate::steps::marker(id, [32., 32.], 32, [21, 154, 195, 255], (1, false), size).unwrap();
    a.kind = kind;
    a.text = None;
    a.step_outline = None;
    a.fill = None;
    a.start = [rect[0], rect[1]];
    a.end = [rect[0] + rect[2], rect[1] + rect[3]];
    a.width = 2;
    a
}
pub fn apply(
    doc: &mut Document,
    path: Option<Path>,
    events: &[Event],
    options: &Options,
) -> Result<()> {
    let mut candidate = doc.clone();
    apply_inner(&mut candidate, path, events, options)?;
    *doc = candidate;
    Ok(())
}
fn apply_inner(
    doc: &mut Document,
    path: Option<Path>,
    events: &[Event],
    options: &Options,
) -> Result<()> {
    if events.windows(2).any(|v| v[0].at > v[1].at)
        || events
            .iter()
            .any(|e| e.point.iter().any(|x| !x.is_finite()))
    {
        return Err(crate::Error::Invalid(
            "Recorded actions contain invalid positions or times.".into(),
        ));
    }
    let Some(mut project) = doc.video.take() else {
        return Ok(());
    };
    let size = [doc.width, doc.height];
    let duration = project.duration();
    let mut id = doc.annotations.iter().map(|a| a.id).max().unwrap_or(0) + 1;
    let first_new = id;
    if options.enabled && options.click_highlights {
        let track = project.add_track();
        project.track_settings_mut(Layer::Annotation(track)).name = "Click highlights".into();
        for (i, event) in events.iter().enumerate().take(1000) {
            let next = events.get(i + 1).map_or(duration, |e| e.at);
            let end = (event.at + 700).min(next).min(duration);
            if end <= event.at {
                continue;
            }
            let mut a = shape(
                id,
                Kind::Ellipse,
                [event.point[0] - 22., event.point[1] - 22., 44., 44.],
                size,
            );
            a.fill = Some([21, 154, 195, 64]);
            a.width = 3;
            let mut timing = Timing::new(track, event.at, end);
            timing.label = "Click highlight".into();
            timing.entrance = Animation::Scale;
            timing.exit = Animation::Fade;
            timing.entrance_ms = 120;
            timing.exit_ms = 250;
            timing.clamp_animations();
            project.place(id, timing);
            doc.annotations.push(a);
            id += 1;
        }
    }
    let mut cursor: Option<Arc<Path>> = None;
    if let Some(mut path) = path.filter(|p| !p.nodes.is_empty() && !p.sprites.is_empty()) {
        path.inputs = events
            .iter()
            .map(|e| Input {
                at: e.at,
                dragging: e.dragging,
                release: e.release.filter(|r| *r >= e.at),
            })
            .collect();
        // The recording is kept, so the reduction can be changed or turned off later.
        path.set_reduction(if options.enabled {
            options.cursor_reduction
        } else {
            Reduction::Off
        });
        path.validate()?;
        let mut a = path.sprites[0].clone().annotation(id);
        a.start = [path.reference[0], path.reference[1]];
        a.end = [
            a.start[0] + path.reference[2],
            a.start[1] + path.reference[3],
        ];
        let track = project.add_track();
        project.track_settings_mut(Layer::Annotation(track)).name = "Cursor".into();
        let mut timing = Timing::new(track, 0, duration);
        timing.label = "Recorded cursor".into();
        let path = Arc::new(path);
        cursor = Some(path.clone());
        timing.motion_path = Some(path);
        project.place(id, timing);
        doc.annotations.push(a);
        id += 1;
    }
    if options.enabled && options.zoom_clicks {
        let first_new = project.zooms.len();
        add_click_zooms(
            &mut project,
            events,
            size,
            options.zoom_cooldown_ms.min(MAX_ZOOM_COOLDOWN_MS),
        );
        if options.zoom_follow_cursor
            && let Some(path) = &cursor
        {
            follow_cursor(&mut project.zooms[first_new..], path, events, size);
        }
        // BB6: the click zooms share zoom tracks instead of one track each.
        let new: Vec<_> = project.zooms[first_new..].iter().map(|z| z.id).collect();
        project.pack_zooms(&new);
    }
    if options.enabled && options.zoom_clicks {
        let mut order = project.layers();
        let mut zooms: Vec<Layer> = vec![];
        for z in &project.zooms {
            if !zooms.contains(&Layer::Zoom(z.track())) {
                zooms.push(Layer::Zoom(z.track()));
            }
        }
        order.retain(|l| !zooms.contains(l));
        project.layer_order = zooms.into_iter().chain(order).collect();
    }
    if options.enabled && options.captions {
        let track = project.add_track();
        project.track_settings_mut(Layer::Annotation(track)).name = "Instructions".into();
        for (i, event) in events.iter().enumerate().take(1000) {
            let end = (event.at + 2500)
                .min(events.get(i + 1).map_or(duration, |e| e.at))
                .min(duration);
            if end <= event.at {
                continue;
            }
            let width = size[0] as f32 * 0.8;
            let height = (size[1] as f32 * 0.12).max(42.);
            let mut a = shape(
                id,
                Kind::Rectangle,
                [
                    (size[0] as f32 - width) * 0.5,
                    size[1] as f32 - height - 20.,
                    width,
                    height,
                ],
                size,
            );
            a.color = [31, 39, 43, 240];
            a.fill = Some(a.color);
            a.corner_radius = 8.;
            a.width = 1;
            a.text = Some(crate::text::TextStyle {
                content: if event.caption.is_empty() {
                    "Click the highlighted control".into()
                } else {
                    event.caption.chars().take(300).collect()
                },
                size_points: 18.,
                color: Some([255; 4]),
                ..Default::default()
            });
            let mut timing = Timing::new(track, event.at, end);
            timing.label = format!("Instruction {}", i + 1);
            timing.entrance = Animation::Fade;
            timing.exit = Animation::Fade;
            timing.entrance_ms = 150;
            timing.exit_ms = 150;
            timing.clamp_animations();
            project.place(id, timing);
            doc.annotations.push(a);
            id += 1;
        }
        // Captions stay above automatic zooms, while cursor and click cues zoom
        // with the underlying recording. Users can freely reorder afterward.
        let mut order = project.layers();
        order.retain(|l| *l != Layer::Annotation(track));
        order.insert(0, Layer::Annotation(track));
        project.layer_order = order;
    }
    // Clicks and the first cursor position can sit at the very edge of the
    // recorded area; keep generated objects on the canvas so the recording opens (BB5).
    for a in doc.annotations.iter_mut().filter(|a| a.id >= first_new) {
        keep_inside(a, size);
    }
    doc.video = Some(project);
    doc.promote_features();
    doc.validate()
}
/// Moves (and if needed trims) a generated object so it lies on the canvas.
fn keep_inside(a: &mut Annotation, size: [u32; 2]) {
    for i in 0..2 {
        let max = size[i] as f32;
        let lo = a.start[i].min(a.end[i]);
        let hi = a.start[i].max(a.end[i]);
        let shift = if lo < 0. {
            -lo
        } else if hi > max {
            max - hi
        } else {
            0.
        };
        for p in [&mut a.start, &mut a.end]
            .into_iter()
            .chain(a.points.iter_mut())
        {
            p[i] = (p[i] + shift).clamp(0., max);
        }
    }
}
/// One editable zoom per click. A click within `cooldown` of the previous one
/// keeps the zoom in and pans to the new target instead of zooming out and back in.
/// Between nearby clicks (a zoom that hands over to a panning zoom), the view
/// follows the cursor as it travels, then the next zoom pans on from wherever
/// the view is. Samples are smoothed so small hand movements don't shake the view.
fn follow_cursor(zooms: &mut [Zoom], cursor: &Path, events: &[Event], size: [u32; 2]) {
    // Pointer tip relative to the cursor image, from the first visible click.
    let tip = events
        .iter()
        .map(|e| (e, cursor.sample(e.at)))
        .find(|(_, n)| n.visible)
        .map_or([0., 0.], |(e, n)| {
            [e.point[0] - n.rect[0], e.point[1] - n.rect[1]]
        });
    for i in 0..zooms.len() {
        let continues = zooms
            .get(i + 1)
            .is_some_and(|n| n.pan_from.is_some() && n.start == zooms[i].end);
        if continues {
            zooms[i].follow = Some(super::zoom::Follow {
                mode: super::zoom::FollowMode::Shown,
                tip,
            });
        }
    }
    refollow(zooms, cursor, 0, size);
}
/// Rebuilds each following zoom's track from the cursor path that starts at
/// project time `start`, in the zoom's Follow mode (BA4). Off clears the track;
/// the next zoom then pans on from this zoom's own center.
pub fn refollow(zooms: &mut [Zoom], cursor: &Path, start: u64, size: [u32; 2]) {
    const STEP: u64 = 100;
    const SETTLE: u64 = 250;
    const SMOOTHING: f32 = 0.35;
    for i in 0..zooms.len() {
        let Some(follow) = zooms[i].follow else {
            continue;
        };
        let continues = zooms
            .get(i + 1)
            .is_some_and(|n| n.pan_from.is_some() && n.start == zooms[i].end);
        let z = &zooms[i];
        let from = z.start + z.entrance_ms + SETTLE;
        if follow.mode == super::zoom::FollowMode::Off || !continues || from + STEP >= z.end {
            let center = z.center;
            zooms[i].track.clear();
            if continues {
                zooms[i + 1].pan_from = Some(center);
            }
            continue;
        }
        let point = |at: u64| {
            let age = at.checked_sub(start)?;
            let n = if follow.mode == super::zoom::FollowMode::Recorded {
                cursor.sample_recorded(age)
            } else {
                cursor.sample(age)
            };
            n.visible.then(|| {
                [
                    ((n.rect[0] + follow.tip[0]) / size[0] as f32).clamp(0., 1.),
                    ((n.rect[1] + follow.tip[1]) / size[1] as f32).clamp(0., 1.),
                ]
            })
        };
        let mut current = z.center;
        let mut track = vec![(from - z.start, current)];
        let mut at = from + STEP;
        while at < z.end && track.len() < 4000 {
            if let Some(p) = point(at) {
                current = [
                    current[0] + (p[0] - current[0]) * SMOOTHING,
                    current[1] + (p[1] - current[1]) * SMOOTHING,
                ];
            }
            track.push((at - z.start, current));
            at += STEP;
        }
        zooms[i].track = track;
        zooms[i + 1].pan_from = Some(current);
    }
}
fn add_click_zooms(
    project: &mut super::editor::Project,
    events: &[Event],
    size: [u32; 2],
    cooldown: u64,
) {
    const LEAD: u64 = 350;
    const HOLD: u64 = 1800;
    let duration = project.duration();
    let mut previous: Option<(usize, u64)> = None; // (zoom index, click time)
    for (i, event) in events.iter().enumerate() {
        if project.zooms.len() >= 64 {
            break;
        }
        let center = [
            (event.point[0] / size[0] as f32).clamp(0., 1.),
            (event.point[1] / size[1] as f32).clamp(0., 1.),
        ];
        let next = events
            .get(i + 1)
            .map_or(duration, |e| e.at.saturating_sub(LEAD));
        let end = (event.at + HOLD).min(next).min(duration);
        let id = project.zooms.iter().map(|z| z.id).max().unwrap_or(0) + 1;
        let continued = previous.filter(|(_, at)| event.at.saturating_sub(*at) <= cooldown);
        if let Some((index, _)) = continued {
            let prev = &project.zooms[index];
            let pan_start = event
                .at
                .saturating_sub(LEAD)
                .max(prev.start + prev.entrance_ms + 1)
                .min(event.at);
            if end > pan_start + 100 {
                let from = prev.center;
                let scale = prev.scale;
                let prev = &mut project.zooms[index];
                prev.end = pan_start;
                prev.exit_ms = 0;
                prev.return_to_full = false;
                let mut z = Zoom::new(id, pan_start, end);
                z.center = center;
                z.scale = scale;
                z.pan_from = Some(from);
                z.entrance_ms = LEAD.min(end - pan_start);
                z.exit_ms = LEAD.min(end - pan_start - z.entrance_ms);
                z.pinned = false;
                project.zooms.push(z);
                previous = Some((project.zooms.len() - 1, event.at));
                continue;
            }
        }
        let start = event.at.saturating_sub(LEAD);
        if end <= start + 100 {
            previous = None;
            continue;
        }
        let mut z = Zoom::new(id, start, end);
        z.center = center;
        z.scale = 1.65;
        z.entrance_ms = LEAD.min(end - start);
        z.exit_ms = LEAD.min(end - start - z.entrance_ms);
        z.pinned = false;
        project.zooms.push(z);
        previous = Some((project.zooms.len() - 1, event.at));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_effects_are_editable_bounded_and_roundtrip() {
        let mut doc = Document::blank(640, 360).unwrap();
        doc.video = Some(super::super::editor::Project::new(
            "test.mp4".into(),
            [640, 360],
            5000,
            30,
        ));
        let events = vec![
            Event {
                at: 600,
                point: [200., 180.],
                button: 0,
                dragging: false,
                release: None,
                caption: "Click Play".into(),
            },
            Event {
                at: 1900,
                point: [420., 160.],
                button: 0,
                dragging: false,
                release: None,
                caption: "Click Settings".into(),
            },
        ];
        apply(
            &mut doc,
            None,
            &events,
            &Options {
                enabled: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(doc.annotations.len(), 4);
        assert_eq!(doc.video.as_ref().unwrap().zooms.len(), 2);
        let copy: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        copy.validate().unwrap();
        assert_eq!(copy.video, doc.video);
        assert!(
            doc.annotations
                .iter()
                .any(|a| a.text.as_ref().is_some_and(|t| t.content == "Click Play"))
        );
    }

    fn click(at: u64, x: f32) -> Event {
        Event {
            at,
            point: [x, 180.],
            button: 0,
            dragging: false,
            release: Some(at + 80),
            caption: String::new(),
        }
    }
    fn zooms_for(events: &[Event], cooldown: u64) -> Vec<Zoom> {
        let mut doc = Document::blank(640, 360).unwrap();
        doc.video = Some(super::super::editor::Project::new(
            "test.mp4".into(),
            [640, 360],
            20_000,
            30,
        ));
        let options = Options {
            enabled: true,
            click_highlights: false,
            captions: false,
            zoom_cooldown_ms: cooldown,
            ..Default::default()
        };
        apply(&mut doc, None, events, &options).unwrap();
        doc.validate().unwrap();
        doc.video.unwrap().zooms
    }
    #[test]
    fn close_clicks_pan_instead_of_zooming_out_and_back_in() {
        let events = [
            click(1000, 100.),
            click(2500, 500.),
            click(3800, 300.),
            click(12_000, 320.),
        ];
        let zooms = zooms_for(&events, DEFAULT_ZOOM_COOLDOWN_MS);
        assert_eq!(zooms.len(), 4);
        // The first three form one continuous zoom: no return to full between them.
        assert!(!zooms[0].return_to_full && !zooms[1].return_to_full);
        assert_eq!(zooms[0].end, zooms[1].start);
        assert_eq!(zooms[1].end, zooms[2].start);
        assert_eq!(zooms[1].pan_from, Some(zooms[0].center));
        // Magnification never drops during the pan.
        for t in (zooms[1].start..zooms[2].end - zooms[2].exit_ms).step_by(50) {
            let z = zooms.iter().find(|z| z.active(t)).unwrap();
            let side = z.view(t).unwrap()[2];
            assert!((side - 1. / 1.65).abs() < 1e-4, "zoomed out at {t}: {side}");
        }
        // A click after the cooldown starts a fresh zoom from the whole frame.
        assert_eq!(zooms[3].pan_from, None);
        assert!(zooms[2].return_to_full);
    }
    #[test]
    fn clicks_and_cursor_at_the_edge_still_open() {
        use crate::video::motion_path::{Node, Purpose};
        let sprite = Arc::new(
            crate::bitmap::Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                12,
                18,
                image::Rgba([0, 0, 0, 255]),
            )))
            .unwrap(),
        );
        let path = Path {
            purpose: Purpose::Cursor,
            reference: [-6., -4., 12., 18.],
            nodes: vec![Node {
                at: 0,
                rect: [-6., -4., 12., 18.],
                visible: true,
                sprite: 0,
                confidence: 1.,
            }],
            sprites: vec![sprite],
            smooth: false,
            enabled: true,
            offset_ms: 0,
            reviewed: true,
            reduction: Reduction::Off,
            inputs: vec![],
            recorded: None,
        };
        let mut doc = Document::blank(640, 360).unwrap();
        doc.video = Some(super::super::editor::Project::new(
            "t.mp4".into(),
            [640, 360],
            6000,
            30,
        ));
        let events = [click(1000, 2.), click(2500, 638.)];
        apply(&mut doc, Some(path), &events, &Options::default()).unwrap();
        doc.validate().unwrap();
    }
    #[test]
    fn zoom_follows_the_cursor_between_nearby_clicks() {
        use crate::video::motion_path::{Node, Purpose};
        let sprite = Arc::new(
            crate::bitmap::Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                12,
                18,
                image::Rgba([0, 0, 0, 255]),
            )))
            .unwrap(),
        );
        // The cursor travels from x=100 to x=500 between clicks at 1000 and 2500.
        let nodes = (0..=40)
            .map(|i| {
                let at = i * 100;
                let x = 100. + (at.clamp(1000, 2500) - 1000) as f32 / 1500. * 400.;
                Node {
                    at,
                    rect: [x, 180., 12., 18.],
                    visible: true,
                    sprite: 0,
                    confidence: 1.,
                }
            })
            .collect();
        let path = Path {
            purpose: Purpose::Cursor,
            reference: [100., 180., 12., 18.],
            nodes,
            sprites: vec![sprite],
            smooth: false,
            enabled: true,
            offset_ms: 0,
            reviewed: true,
            reduction: Reduction::Off,
            inputs: vec![],
            recorded: None,
        };
        let events = [click(1000, 100.), click(2500, 500.)];
        let build = |follow: bool| {
            let mut doc = Document::blank(640, 360).unwrap();
            doc.video = Some(super::super::editor::Project::new(
                "t.mp4".into(),
                [640, 360],
                6000,
                30,
            ));
            let options = Options {
                enabled: true,
                click_highlights: false,
                captions: false,
                cursor_reduction: Reduction::Off,
                zoom_follow_cursor: follow,
                ..Default::default()
            };
            apply(&mut doc, Some(path.clone()), &events, &options).unwrap();
            doc.validate().unwrap();
            doc.video.unwrap().zooms
        };
        let still = build(false);
        let follow = build(true);
        assert!(still.iter().all(|z| z.track.is_empty()));
        assert!(!follow[0].track.is_empty());
        // Mid-travel, the followed view has moved right toward the cursor.
        let mid = 1900;
        let x = |zooms: &[Zoom]| {
            zooms
                .iter()
                .find(|z| z.active(mid))
                .unwrap()
                .view(mid)
                .unwrap()[0]
        };
        assert!(
            x(&follow) > x(&still) + 0.02,
            "{} vs {}",
            x(&follow),
            x(&still)
        );
        // The next zoom pans on from where the followed view ended: no jump.
        assert_eq!(follow[1].pan_from, Some(follow[0].track.last().unwrap().1));
        let cut = follow[0].end;
        let before = follow[0].view(cut - 1).unwrap();
        let after = follow[1].view(cut).unwrap();
        assert!((before[0] - after[0]).abs() < 0.01 && (before[1] - after[1]).abs() < 0.01);
    }
    #[test]
    fn zero_cooldown_keeps_separate_zooms() {
        let events = [click(1000, 100.), click(2500, 500.)];
        let zooms = zooms_for(&events, 0);
        assert!(zooms.iter().all(|z| z.pan_from.is_none()));
    }
}
