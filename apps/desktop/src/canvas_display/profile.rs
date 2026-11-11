use super::*;
#[test]
#[ignore = "synthetic performance profile; opt in explicitly, no windows or clipboard"]
fn guide_interaction_profile() {
    use capture_core::step_capture::{Button, Gesture, Guide};
    use std::time::Instant;
    let mut rows = vec![];
    for count in [0, 1, 5, 6] {
        let ordinary = count == 0 || count == 6;
        let mut guide = Guide::new([1920, 1080]).unwrap();
        for n in 0..if ordinary { 0 } else { count } {
            let pixels = image::RgbaImage::from_fn(1920, 1080, |x, y| {
                image::Rgba([
                    ((x / 8 + n * 17) % 256) as u8,
                    ((y / 7) % 256) as u8,
                    ((x + y) / 13 % 256) as u8,
                    255,
                ])
            });
            guide
                .push(
                    pixels,
                    Gesture {
                        start: [200, 300],
                        end: [200, 300],
                        button: Button::Left,
                    },
                )
                .unwrap();
        }
        let doc = if ordinary {
            let pixels = image::RgbaImage::from_fn(1920, 1080, |x, y| {
                image::Rgba([(x % 251) as u8, (y % 241) as u8, 180, 255])
            });
            let mut d = if count == 6 {
                capture_core::Document::from_capture_image(pixels).unwrap()
            } else {
                capture_core::Document::from_image(pixels).unwrap()
            };
            d.annotations.push(
                capture_core::steps::marker(
                    2,
                    [300., 200.],
                    64,
                    [40, 100, 200, 255],
                    (1, false),
                    [1920, 1080],
                )
                .unwrap(),
            );
            d
        } else {
            guide.document().unwrap()
        };
        let mut session = capture_core::Session::new(doc).unwrap();
        if !ordinary {
            let moved = capture_core::canvas_expansion::translated(
                &session.document.annotations[..1],
                [100., 0.],
            )
            .unwrap();
            session
                .execute(capture_core::Command::MoveAndExpand {
                    annotations: moved,
                    duplicate: false,
                })
                .unwrap();
            use capture_core::guide_sections::{Action, SectionKind};
            session
                .execute(capture_core::Command::Guide {
                    action: Action::Insert {
                        after: Some(1),
                        kind: SectionKind::Text,
                    },
                })
                .unwrap();
            session
                .execute(capture_core::Command::Guide {
                    action: Action::Remove {
                        id: u64::from(count) + 1,
                    },
                })
                .unwrap();
        }
        let doc = session.document;
        assert!(crate::guide_display::supported(&doc));
        let list = doc.display_list().unwrap();
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut scene = crate::guide_display::Scene::default();
        for zoom in [0.85, 1., 1.2] {
            let t = Transform {
                viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 600.)),
                image: egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(doc.width as f32, doc.height as f32) * zoom,
                ),
                scale: zoom,
                zoom,
            };
            let key = cached_viewport(t, 1., 8192, None);
            let start = Instant::now();
            for _ in 0..3 {
                std::hint::black_box(list.render_viewport(key).unwrap());
            }
            let zoom_ms = start.elapsed().as_secs_f64() * 1000. / 3.;
            let mut moved = doc
                .annotations
                .iter()
                .find(|a| a.kind == crate::Kind::Image || count == 0)
                .unwrap()
                .clone();
            moved.start[0] += 10.;
            moved.end[0] += 10.;
            let start = Instant::now();
            for _ in 0..3 {
                let plan = doc
                    .plan_expanding_move(std::slice::from_ref(&moved), false)
                    .unwrap();
                let list = plan.display_list(&doc).unwrap();
                std::hint::black_box(list.render_viewport(key).unwrap());
            }
            let move_ms = start.elapsed().as_secs_f64() * 1000. / 3.;
            let mut paint = |document: &crate::Document, transform: Transform| {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(t.viewport),
                        ..Default::default()
                    },
                    |ui| scene.paint(ui, document, transform).unwrap(),
                );
                out.textures_delta.clear();
            };
            paint(&doc, t);
            let start = Instant::now();
            for n in 0..6 {
                let z = zoom + n as f32 * 0.01;
                let mut t = t;
                t.scale = z;
                t.zoom = z;
                t.image = egui::Rect::from_min_size(
                    t.image.min,
                    egui::vec2(doc.width as f32, doc.height as f32) * z,
                );
                paint(&doc, t);
            }
            let retained_zoom_ms = start.elapsed().as_secs_f64() * 1000. / 6.;
            paint(&doc, t);
            let start = Instant::now();
            for n in 0..12 {
                let mut moved = moved.clone();
                moved.start[0] += n as f32 * 0.37;
                moved.end[0] += n as f32 * 0.37;
                let plan = doc.plan_expanding_move(&[moved], false).unwrap();
                let mut d = doc.clone();
                d.annotations = plan.annotations;
                paint(&d, t);
            }
            let retained_move_ms = start.elapsed().as_secs_f64() * 1000. / 12.;
            rows.push(serde_json::json!({"stage":if count==0 {"ordinary raster capture with annotation"} else if count==6 {"editable screenshot capture"} else {"after canvas expansion and section insert/delete"},"screenshots":if ordinary {1}else{count},"zoom":zoom,"rendered_pixels":key.size[0] as u64*key.size[1] as u64,"legacy_zoom_ms":zoom_ms,"legacy_move_ms":move_ms,"retained_zoom_ms":retained_zoom_ms,"retained_move_ms":retained_move_ms,"measurement":"CPU frame preparation only; excludes GPU upload/presentation"}));
        }
    }
    let report = serde_json::to_string_pretty(&rows).unwrap();
    println!("{report}");
    if let Ok(path) = std::env::var("CLIPDESIGNER_GUIDE_PROFILE") {
        std::fs::write(path, report).unwrap();
    }
}
