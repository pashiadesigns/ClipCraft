use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
fn focus_drag_resize_rotation_and_zoom_reuse_lower_pixels() {
    for blur in [0., 8.] {
        let mut d = fixture([1920, 1080], Kind::Rectangle);
        d.annotations[1].focus = Some(capture_core::focus::Focus {
            blur,
            ..Default::default()
        });
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        let mut first = paint(&ctx, &mut scene, &d, 0.7);
        let key = scene.effects[&2].tiles[0].1.id();
        let clear = scene.effects[&2].clear.as_ref().unwrap()[0].1.id();
        let now = std::time::Instant::now();
        for i in 0..40 {
            d.annotations[1].start[0] += 1.25;
            d.annotations[1].end[0] += 1.75;
            d.annotations[1].rotation_degrees = i as f32;
            d.annotations[1].corner_radius = i as f32;
            let frame = paint(&ctx, &mut scene, &d, 0.7 + i as f32 * 0.001);
            first
                .0
                .textures_delta
                .append(frame.0.textures_delta.clone());
            assert_eq!(
                scene.effects[&2].tiles[0].1.id(),
                key,
                "drag rebuilt the effect"
            );
            assert_eq!(scene.effects[&2].clear.as_ref().unwrap()[0].1.id(), clear);
        }
        eprintln!(
            "Focus blur {blur}: retained drag CPU {:.3} ms/update",
            now.elapsed().as_secs_f64() * 1000. / 40.
        );
        if let Ok(root) = std::env::var("CLIPCRAFT_PUBLIC_UI") {
            let last = paint(&ctx, &mut scene, &d, 0.7);
            dump_meshes(&ctx, &root, &format!("focus-drag-{blur}"), first, last);
            d.render()
                .unwrap()
                .save(std::path::Path::new(&root).join(format!("focus-exact-{blur}.png")))
                .unwrap();
        }
        d.annotations[0].start[0] += 5.;
        paint(&ctx, &mut scene, &d, 0.7);
        assert_ne!(
            scene.effects[&2].tiles[0].1.id(),
            key,
            "changed lower pixels were stale"
        );
    }
}

#[test]
fn reduced_crossfade_surface_keeps_background_and_upper_annotations() {
    let ctx = egui::Context::default();
    let mut scene = Scene::default();
    let mut d = Document::blank(1920, 1080).unwrap();
    let project =
        capture_core::video::editor::Project::new("synthetic.mp4".into(), [1920, 1080], 4000, 30);
    let pixels = std::sync::Arc::new(image::RgbaImage::from_pixel(
        480,
        270,
        image::Rgba([25, 130, 210, 255]),
    ));
    let layer = std::sync::Arc::new(capture_core::bitmap::Bitmap::transient(pixels).unwrap());
    d.video = Some(project.clone());
    d.annotations = capture_core::video::frame::ordered_objects(
        &d,
        &[(project.media_tracks[0], layer)],
        1000,
        false,
    );
    let mut top = capture_core::steps::marker(
        5,
        [400., 200.],
        80,
        [220, 30, 40, 255],
        (1, false),
        [1920, 1080],
    )
    .unwrap();
    top.kind = Kind::Rectangle;
    top.text = None;
    top.fill = Some(top.color);
    d.annotations.push(top);
    d.video = None;
    let mut draw = || {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960., 540.),
                )),
                ..Default::default()
            },
            |ui| {
                scene
                    .paint_video(
                        ui,
                        &d,
                        Transform {
                            viewport: ui.clip_rect(),
                            image: egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(960., 540.),
                            ),
                            scale: 0.5,
                            zoom: 0.5,
                        },
                        None,
                        1,
                        &project,
                    )
                    .unwrap();
            },
        ))
    };
    let first = draw();
    let last = draw();
    assert!(scene.vectors.contains_key(&5));
    assert!(!scene.images.is_empty());
    if let Ok(root) = std::env::var("CLIPCRAFT_PUBLIC_UI") {
        dump_meshes(&ctx, &root, "reduced-crossfade-upper", first, last);
    }
}

fn fixture(size: [u32; 2], kind: Kind) -> Document {
    let mut d =
        Document::from_capture_image(image::RgbaImage::from_fn(size[0], size[1], |x, y| {
            image::Rgba([
                ((x / 8) % 2 * 160 + 50) as u8,
                ((y / 8) % 2 * 140 + 60) as u8,
                180,
                255,
            ])
        }))
        .unwrap();
    let mut a =
        capture_core::steps::marker(2, [200., 140.], 60, [60, 100, 200, 255], (1, false), size)
            .unwrap();
    a.kind = kind;
    a.start = [100., 80.];
    a.end = [340., 220.];
    a.width = 8;
    a.text = None;
    a.fill = None;
    if kind == Kind::Eraser {
        a.points = vec![a.start, [160., 170.], a.end];
        a.width = 32;
    }
    d.annotations.push(a);
    let mut top =
        capture_core::steps::marker(3, [420., 300.], 48, [200, 40, 60, 255], (2, false), size)
            .unwrap();
    top.kind = Kind::Rectangle;
    top.text = None;
    top.fill = Some([200, 40, 60, 255]);
    d.annotations.push(top);
    d.promote_features();
    d
}
fn paint(ctx: &egui::Context, scene: &mut Scene, d: &Document, zoom: f32) -> PaintedOutput {
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900., 600.),
            )),
            ..Default::default()
        },
        |ui| {
            // Checkerboard makes erased alpha visible in the actual paint output.
            for y in (0..600).step_by(12) {
                for x in (0..900).step_by(12) {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x as f32, y as f32),
                            egui::vec2(12., 12.),
                        ),
                        0.,
                        egui::Color32::from_gray(if (x + y) / 12 % 2 == 0 { 210 } else { 245 }),
                    );
                }
            }
            scene
                .paint(
                    ui,
                    d,
                    Transform {
                        viewport: ui.clip_rect(),
                        image: egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(d.width as f32, d.height as f32) * zoom,
                        ),
                        scale: zoom,
                        zoom,
                    },
                )
                .unwrap();
        },
    ))
}
#[test]
fn page_cut_flatten_undo_restores_retained_text_shapes_and_frames() {
    use capture_core::{Command, Session, cut_out::CutAxis, guide_sections::Action, templates};
    let ctx = egui::Context::default();
    let mut scene = Scene::default();
    let mut session =
        Session::new(templates::build("three-steps", "Guide", &[], templates::Scale::Fit).unwrap())
            .unwrap();
    session
        .execute(Command::Guide {
            action: Action::AddPage {
                page: 0,
                copy: true,
            },
        })
        .unwrap();
    let mut all = paint(&ctx, &mut scene, &session.document, 0.5);
    let before = paint(&ctx, &mut scene, &session.document, 0.5);
    let vector_count = scene.vectors.len();
    assert!(vector_count > 6);
    if let Ok(root) = std::env::var("CLIPCRAFT_PAGE_UNDO_UI") {
        dump_meshes(&ctx, &root, "Before", PaintedOutput(all.0.clone()), before);
    }
    for command in [
        Command::CutOutPages {
            axis: CutAxis::Horizontal,
            start: 180,
            end: 200,
            page: 0,
            all_pages: false,
        },
        Command::FlattenPage { page: 0 },
        Command::Undo,
        Command::Undo,
    ] {
        session.execute(command).unwrap();
        let frame = paint(&ctx, &mut scene, &session.document, 0.5);
        all.0.textures_delta.append(frame.0.textures_delta.clone());
    }
    let after = paint(&ctx, &mut scene, &session.document, 0.5);
    assert_eq!(scene.vectors.len(), vector_count);
    if let Ok(root) = std::env::var("CLIPCRAFT_PAGE_UNDO_UI") {
        dump_meshes(&ctx, &root, "After", all, after);
    }
}
#[test]
fn effects_retain_pixels_across_zoom_and_unrelated_edits_but_invalidate_dependencies() {
    for kind in [Kind::Blur, Kind::Pixelate, Kind::Eraser, Kind::Redact] {
        let mut d = fixture([640, 400], kind);
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        let first = paint(&ctx, &mut scene, &d, 1.);
        let still = paint(&ctx, &mut scene, &d, 1.);
        assert!(still.0.textures_delta.set.is_empty());
        if let Ok(root) = std::env::var("CLIPCRAFT_EFFECTS_UI") {
            dump_meshes(&ctx, &root, &format!("{kind:?}"), first, still);
        }
        let id = scene.effects[&2].tiles[0].1.id();
        for zoom in [0.79, 0.97, 1., 1.25] {
            paint(&ctx, &mut scene, &d, zoom);
            assert_eq!(
                scene.effects[&2].tiles[0].1.id(),
                id,
                "zoom rebuilt effect pixels"
            );
            d.annotations[2].start[0] += 2.;
            d.annotations[2].end[0] += 2.;
            let out = paint(&ctx, &mut scene, &d, zoom);
            assert!(
                out.0.textures_delta.set.is_empty(),
                "unrelated drag rebuilt pixels"
            );
        }
        d.annotations[0].start[0] += 10.;
        d.annotations[0].end[0] -= 10.;
        paint(&ctx, &mut scene, &d, 1.);
        assert_ne!(
            scene.effects[&2].tiles[0].1.id(),
            id,
            "underlying screenshot change did not invalidate patch"
        );
        let rendered = d.render().unwrap();
        let saved = serde_json::to_vec(&d).unwrap();
        let reopened: Document = serde_json::from_slice(&saved).unwrap();
        assert_eq!(reopened.render().unwrap(), rendered);
        paint(&ctx, &mut scene, &reopened, 0.85);
        assert_eq!(scene.effects.len(), 1);
        d.annotations.remove(1);
        paint(&ctx, &mut scene, &d, 0.85);
        assert!(scene.effects.is_empty());
    }
}
#[test]
#[ignore = "optimized synthetic CPU profile; no desktop access"]
fn pixel_effect_interaction_profile() {
    use std::time::Instant;
    let mut rows = vec![];
    for kind in [Kind::Blur, Kind::Eraser, Kind::Pixelate] {
        let d = fixture([1920, 1080], kind);
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        for zoom in [0.79, 0.97, 1., 1.25] {
            paint(&ctx, &mut scene, &d, zoom);
            let start = Instant::now();
            let list = d.display_list().unwrap();
            let _ = list
                .render_viewport(Viewport {
                    size: [900, 600],
                    origin: [0., 0.],
                    scale: zoom as f64,
                })
                .unwrap();
            let legacy = start.elapsed().as_secs_f64() * 1000.;
            let mut times = vec![];
            for what in ["zoom", "overlay drag", "effect drag", "screenshot drag"] {
                let start = Instant::now();
                for n in 1..=8 {
                    let mut moved = d.clone();
                    let index = match what {
                        "overlay drag" => Some(2),
                        "effect drag" => Some(1),
                        "screenshot drag" => Some(0),
                        _ => None,
                    };
                    if let Some(i) = index {
                        moved.annotations[i] = capture_core::canvas_expansion::translated(
                            &d.annotations[i..=i],
                            [n as f32 * 0.37, 0.],
                        )
                        .unwrap()
                        .remove(0);
                    }
                    paint(
                        &ctx,
                        &mut scene,
                        &moved,
                        if what == "zoom" {
                            zoom + n as f32 * 0.002
                        } else {
                            zoom
                        },
                    );
                }
                times.push((what, start.elapsed().as_secs_f64() * 1000. / 8.));
            }
            rows.push(serde_json::json!({"effect":format!("{kind:?}"),"zoom":zoom,"legacy_rebuild_ms":legacy,"retained_ms":times,"measurement":"CPU preparation; excludes GPU upload/presentation"}));
        }
    }
    std::fs::create_dir_all("../../artifacts/pixel-effects-fix").unwrap();
    std::fs::write(
        "../../artifacts/pixel-effects-fix/performance.json",
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}
