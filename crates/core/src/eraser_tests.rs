use crate::{Annotation, Command, Document, Kind, Session};
use image::{Rgba, RgbaImage};

fn eraser(opacity: u8) -> Annotation {
    Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,

        focus: None,

        highlighter_rect: false,
        hard_edge: false,
        group: None,
        rotation_degrees: 0.0,
        mirror_x: false,
        id: 1,
        kind: Kind::Eraser,
        start: [10.0, 20.0],
        end: [10.0, 20.0],
        points: vec![[10.0, 20.0], [50.0, 20.0], [10.0, 20.0]],
        color: [250, 20, 180, opacity], // RGB must not tint erasure.
        width: 8,
        corner_radius: 0.0,
        fill: None,
        text: None,
        callout: None,
    }
}

#[test]
fn eraser_alpha_coverage_is_once_per_stroke_and_preserves_outside_pixels() {
    for source_alpha in [0, 128, 255] {
        let source = RgbaImage::from_pixel(64, 48, Rgba([25, 100, 210, source_alpha]));
        for opacity in [0, 128, 255] {
            let mut document = Document::from_image(source.clone()).unwrap();
            document.annotations.push(eraser(opacity));
            let output = document.render().unwrap();
            let alpha = ((u16::from(source_alpha) * (255 - u16::from(opacity)) + 127) / 255) as u8;
            assert_eq!(output.get_pixel(30, 20)[3], alpha);
            if opacity != 0 {
                assert_eq!(
                    output.get_pixel(30, 20).0,
                    if alpha == 0 {
                        [0; 4]
                    } else {
                        [25, 100, 210, alpha]
                    }
                );
            } else {
                assert_eq!(output, source);
            }
            // Half a pixel inside the edge of a soft eraser that fades over
            // 60% of its 4 px radius.
            let edge = ((0.5f64 / 2.4) * 255.0).round() as u16;
            let edge_opacity = (u16::from(opacity) * edge + 127) / 255;
            assert_eq!(
                output.get_pixel(30, 16)[3],
                ((u16::from(source_alpha) * (255 - edge_opacity) + 127) / 255) as u8
            );
            for y in 0..48 {
                for x in 0..64 {
                    if !(6..55).contains(&x) || !(16..25).contains(&y) {
                        assert_eq!(output.get_pixel(x, y), source.get_pixel(x, y));
                    }
                }
            }
        }
    }
}

#[test]
fn eraser_stacking_moves_history_and_real_project_roundtrip() {
    let mut document = Document::canvas(64, 48, [30, 80, 200, 255]).unwrap();
    let mut cover = eraser(255);
    cover.kind = Kind::Redact;
    cover.points.clear();
    cover.start = [20.0, 10.0];
    cover.end = [40.0, 30.0];
    document.annotations.push(cover);
    let before = document.render().unwrap();
    let source = document.source_png.clone();
    let mut session = Session::new(document).unwrap();
    let mut brush = eraser(255);
    brush.id = 2;
    session
        .execute(Command::Add {
            annotation: brush.clone(),
        })
        .unwrap();
    let erased = session.document.render().unwrap();
    assert_eq!(erased.get_pixel(30, 20).0, [0; 4]);
    assert_eq!(erased.get_pixel(30, 12).0, [0, 0, 0, 255]);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.render().unwrap(), before);
    assert!(!session.can_undo());
    session.execute(Command::Redo).unwrap();
    let moved = brush.translated([0.0, 16.0], [64, 48]).unwrap();
    session
        .execute(Command::Update { annotation: moved })
        .unwrap();
    assert_eq!(
        session.document.render().unwrap().get_pixel(30, 20).0,
        [0, 0, 0, 255]
    );
    session.execute(Command::Undo).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("eraser.capture.json");
    let png = dir.path().join("eraser.png");
    session.save(&project).unwrap();
    let loaded = Document::load(&project).unwrap();
    assert_eq!(loaded.source_png, source);
    assert_eq!(loaded.annotations[1], brush);
    assert_eq!(loaded.render().unwrap(), erased);
    loaded.export_png(&png).unwrap();
    assert_eq!(image::open(&png).unwrap().into_rgba8(), erased);
    let mut on_top = eraser(255);
    on_top.id = 3;
    on_top.kind = Kind::Pen;
    on_top.color = [90, 180, 50, 255];
    session
        .execute(Command::Add { annotation: on_top })
        .unwrap();
    assert_eq!(
        session.document.render().unwrap().get_pixel(30, 20).0,
        [90, 180, 50, 255]
    );
}

#[test]
fn eraser_dot_clips_edges_and_invalid_paths_preserve_history() {
    let mut session = Session::new(Document::blank(8, 8).unwrap()).unwrap();
    let mut dot = eraser(255);
    dot.points = vec![[0.0, 0.0]; 2];
    dot.start = [0.0, 0.0];
    dot.end = [0.0, 0.0];
    session
        .execute(Command::Add {
            annotation: dot.clone(),
        })
        .unwrap();
    assert_eq!(session.document.render().unwrap().get_pixel(0, 0).0, [0; 4]);
    assert_eq!(
        session.document.render().unwrap().get_pixel(7, 7).0,
        [255; 4]
    );
    session.execute(Command::Undo).unwrap();
    for bad in [
        vec![],
        vec![[0.0, 0.0]],
        vec![[0.0, 0.0], [f32::NAN, 1.0]],
        vec![[0.0, 0.0]; 8193],
    ] {
        dot.points = bad;
        assert!(
            session
                .execute(Command::Add {
                    annotation: dot.clone()
                })
                .is_err()
        );
        assert!(session.document.annotations.is_empty());
        assert!(session.can_redo());
    }
}

#[test]
fn a_hard_eraser_leaves_a_clean_edge_and_a_soft_one_fades() {
    let partial = |hard: bool| {
        let mut doc = Document::canvas(80, 40, [200, 40, 40, 255]).unwrap();
        let mut a = eraser(255);
        a.width = 16;
        a.hard_edge = hard;
        a.start = [10., 20.];
        a.end = [70., 20.];
        a.points = vec![[10., 20.], [70., 20.]];
        doc.annotations.push(a);
        let image = doc.render().unwrap();
        image.pixels().filter(|p| p[3] != 0 && p[3] != 255).count()
    };
    let (soft, hard) = (partial(false), partial(true));
    assert_eq!(hard, 0, "Hard edge has no partly erased pixels");
    assert!(
        soft > 150,
        "the soft edge fades across several pixels: {soft}"
    );
}

/// Timing probe for the beta 4 "Eraser makes the canvas laggy" report:
/// `cargo test --release -p capture-core eraser_patch_timing -- --ignored --nocapture`
#[test]
#[ignore = "timing probe"]
fn eraser_patch_timing() {
    use crate::display::effect_patch::Plan;
    let (w, h) = (1600u32, 1000u32);
    let base = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 251) as u8, (y % 241) as u8, 128, 255])
    });
    let mut objects = Vec::new();
    for i in 0..12u64 {
        let mut a = eraser(255);
        a.id = 100 + i;
        a.kind = Kind::Rectangle;
        a.points.clear();
        a.start = [80.0 + 120.0 * i as f32, 120.0];
        a.end = [180.0 + 120.0 * i as f32, 860.0];
        a.fill = Some([40, 90, 160, 255]);
        objects.push(a);
    }
    // A long scribble across most of the page, like erasing a few lines.
    let stroke = |id: u64, n: usize| {
        let mut a = eraser(255);
        a.id = id;
        a.width = 24;
        a.points = (0..n)
            .map(|k| {
                let t = k as f32 / n as f32;
                [
                    100.0 + 1400.0 * t,
                    200.0 + 600.0 * t + 80.0 * (t * 40.0).sin(),
                ]
            })
            .collect();
        a.start = a.points[0];
        a.end = *a.points.last().unwrap();
        a
    };
    for (strokes, with_objects) in [(1usize, true), (5, true), (5, false)] {
        let mut all = if with_objects {
            objects.clone()
        } else {
            Vec::new()
        };
        for s in 0..strokes {
            all.push(stroke(1 + s as u64, 600));
        }
        let start = std::time::Instant::now();
        let mut area = 0u64;
        for i in 0..all.len() {
            if let Some(plan) = Plan::new(&all, i, [w, h]) {
                for cell in plan.changed_tiles(&all[i], 128) {
                    area += u64::from(cell[2]) * u64::from(cell[3]);
                    let _ = plan
                        .tile(cell, [w, h])
                        .unwrap()
                        .render(&base, [0, 0])
                        .unwrap();
                }
            }
        }
        println!(
            "{strokes} stroke(s), objects {with_objects}: all patches {:.1} ms, patch area {:.1} Mpx (page {:.1} Mpx)",
            start.elapsed().as_secs_f64() * 1000.0,
            area as f64 / 1e6,
            f64::from(w * h) / 1e6
        );
    }
    // Dragging one object under five strokes: only tiles whose sub-plan changed
    // are redrawn (the display's per-tile cache).
    let parts = |all: &[Annotation]| {
        let mut out = Vec::new();
        for i in 0..all.len() {
            if let Some(plan) = Plan::new(all, i, [w, h]) {
                for cell in plan.changed_tiles(&all[i], 128) {
                    out.push((all[i].id, cell, plan.tile(cell, [w, h]).unwrap()));
                }
            }
        }
        out
    };
    let mut all = objects.clone();
    for s in 0..5 {
        all.push(stroke(1 + s as u64, 600));
    }
    let before = parts(&all);
    all[3].start[0] += 10.0;
    all[3].end[0] += 10.0;
    let start = std::time::Instant::now();
    let after = parts(&all);
    let mut redrawn = 0;
    for (id, cell, part) in &after {
        if !before
            .iter()
            .any(|(i, c, p)| i == id && c == cell && p == part)
        {
            let _ = part.render(&base, [0, 0]).unwrap();
            redrawn += 1;
        }
    }
    println!(
        "drag one object under 5 strokes: {redrawn} of {} tiles redrawn, {:.1} ms per frame",
        after.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    // Live drawing: each pointer move re-plans and re-renders the growing stroke.
    let mut all = objects.clone();
    let full = stroke(1, 600);
    let start = std::time::Instant::now();
    let mut frames = 0;
    for n in (20..=600).step_by(20) {
        let mut a = full.clone();
        a.points.truncate(n);
        all.truncate(objects.len());
        all.push(a);
        let plan = Plan::new(&all, all.len() - 1, [w, h]).unwrap();
        for cell in plan.changed_tiles(all.last().unwrap(), 128) {
            let _ = plan
                .tile(cell, [w, h])
                .unwrap()
                .render(&base, [0, 0])
                .unwrap();
        }
        frames += 1;
    }
    println!(
        "live stroke: {frames} frames, {:.1} ms per frame",
        start.elapsed().as_secs_f64() * 1000.0 / frames as f64
    );
}

/// Displays replace only `changed_tiles` of an eraser; every other pixel of its
/// bounding box must be untouched by the stroke, and each tile's own render must
/// equal the full render there (soft and hard edges, partial opacity).
#[test]
fn eraser_changed_tiles_cover_every_changed_pixel_and_render_identically() {
    use crate::display::effect_patch::Plan;
    let (w, h) = (420u32, 300u32);
    let base = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 200) as u8, (y % 190) as u8, 90, 255])
    });
    for (hard, opacity) in [(false, 255), (true, 255), (false, 140)] {
        let mut box_ = eraser(255);
        box_.id = 7;
        box_.kind = Kind::Rectangle;
        box_.points.clear();
        box_.start = [30.0, 40.0];
        box_.end = [390.0, 260.0];
        box_.fill = Some([200, 60, 40, 255]);
        let mut stroke = eraser(opacity);
        stroke.id = 9;
        stroke.hard_edge = hard;
        stroke.width = 18;
        stroke.points = (0..80)
            .map(|k| {
                let t = k as f32 / 79.0;
                [20.0 + 380.0 * t, 40.0 + 220.0 * t + 30.0 * (t * 18.0).sin()]
            })
            .collect();
        stroke.start = stroke.points[0];
        stroke.end = *stroke.points.last().unwrap();
        let mut before = Document::from_image(base.clone()).unwrap();
        before.annotations.push(box_.clone());
        let mut after = before.clone();
        after.annotations.push(stroke.clone());
        let (without, with) = (before.render().unwrap(), after.render().unwrap());
        let objects = vec![box_, stroke.clone()];
        let plan = Plan::new(&objects, 1, [w, h]).unwrap();
        let tiles = plan.changed_tiles(&stroke, 64);
        assert!(
            tiles.len() > 1,
            "a long diagonal stroke spans several tiles"
        );
        let [ox, oy, ow, oh] = plan.output;
        let covered: u64 = tiles
            .iter()
            .map(|t| u64::from(t[2]) * u64::from(t[3]))
            .sum();
        assert!(
            covered < u64::from(ow) * u64::from(oh),
            "tiles must skip untouched parts of the box"
        );
        let inside = |x: u32, y: u32| {
            tiles
                .iter()
                .any(|t| x >= t[0] && y >= t[1] && x < t[0] + t[2] && y < t[1] + t[3])
        };
        for y in oy..oy + oh {
            for x in ox..ox + ow {
                if !inside(x, y) {
                    assert_eq!(
                        with.get_pixel(x, y),
                        without.get_pixel(x, y),
                        "eraser changed ({x},{y}) outside its tiles (hard {hard}, opacity {opacity})"
                    );
                }
            }
        }
        for cell in tiles {
            let pixels = plan
                .tile(cell, [w, h])
                .unwrap()
                .render(&base, [0, 0])
                .unwrap();
            for y in 0..cell[3] {
                for x in 0..cell[2] {
                    assert_eq!(
                        pixels.get_pixel(x, y),
                        with.get_pixel(cell[0] + x, cell[1] + y),
                        "tile {cell:?} differs at ({x},{y})"
                    );
                }
            }
        }
    }
}
