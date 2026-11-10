use super::repair_tests::{editor, press};
use super::*;
use capture_core::video::editor::Audio;

#[test]
fn remaining_properties_contexts_use_current_headers_and_fit_narrow_panes() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    for width in [260., 390.] {
        for scene in [
            "cut-out",
            "video-project",
            "video-tool",
            "video-icon",
            "text-edit",
            "video-privacy",
        ] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            match scene {
                "cut-out" => {
                    e.session.document.video = None;
                    e.begin_cut_out();
                }
                "video-tool" => {
                    e.tool = Some(crate::Kind::Redact);
                }
                "video-icon" => {
                    let a = &mut e.session.document.annotations[0];
                    let mut icon = std::sync::Arc::new(
                        capture_core::bitmap::Bitmap::from_vector(
                            capture_core::icon_library::vector::VectorIcon::builtin("check-circle"),
                        )
                        .unwrap(),
                    )
                    .annotation(a.id);
                    icon.start = a.start;
                    icon.end = a.end;
                    *a = icon;
                    e.select_only(Some(1));
                }
                "text-edit" => {
                    e.select_only(Some(1));
                    e.begin_text_edit();
                }
                "video-privacy" => {
                    e.open_privacy_review();
                }
                _ => (),
            }
            let mut draw = || {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(
                                if scene == "video-privacy" {
                                    900.
                                } else {
                                    width
                                },
                                900.,
                            ),
                        )),
                        focused: true,
                        ..Default::default()
                    },
                    |ui| {
                        if scene == "video-privacy" {
                            e.show_privacy_review(ui.ctx());
                        } else if scene == "cut-out" {
                            e.image_properties_pane(ui);
                        } else {
                            e.video_properties_pane(ui);
                        }
                    },
                ))
            };
            let mut first = draw();
            for _ in 0..3 {
                let out = draw();
                for (id, delta) in &out.0.textures_delta.set {
                    first
                        .0
                        .textures_delta
                        .set
                        .entry(*id)
                        .or_default()
                        .extend(delta.clone());
                }
            }
            let last = draw();
            assert!(!last.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgb(60,67,69))), "Old header in {scene}");
            if let Some(folder) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
                dump_meshes(
                    &ctx,
                    &folder.to_string_lossy(),
                    &format!("beta-{scene}-{width}"),
                    first,
                    last,
                );
            }
        }
    }
}

#[test]
fn middle_drag_pans_timeline_without_selection_or_project_edit() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    for _ in 0..20 {
        e.session.document.video.as_mut().unwrap().add_track();
    }
    e.set_timeline_expanded(true);
    super::repair_tests::render(&ctx, &mut e, vec![]);
    super::repair_tests::render(&ctx, &mut e, vec![]);
    let before = e.session.document.clone();
    let panel: egui::Rect = ctx
        .data(|d| d.get_temp(egui::Id::unique("timeline-panel")))
        .unwrap();
    let from = egui::pos2(panel.center().x, panel.bottom() - 35.);
    let to = from - egui::vec2(100., 60.);
    let button = |p, pressed| {
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Middle,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    super::repair_tests::render(&ctx, &mut e, button(from, true));
    let output = super::repair_tests::render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Move);
    assert!(
        e.video.timeline_offset.x > 10. && e.video.timeline_offset.y > 10.,
        "{:?}",
        e.video.timeline_offset
    );
    assert!(e.video.marquee.is_none());
    assert_eq!(e.session.document.video, before.video);
    assert_eq!(e.session.document.annotations, before.annotations);
    super::repair_tests::render(&ctx, &mut e, button(to, false));
}

#[test]
fn refinement_production_layouts_fit_and_render() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    for scene in ["shadow", "pen", "icons", "capture"] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        let mut e = editor(&ctx);
        e.session.document.video = None;
        e.select_only(Some(1));
        e.tool = None;
        if scene == "shadow" {
            e.session.document.annotations[0].shadow = Some(Default::default());
        }
        if scene == "pen" {
            let a = &mut e.session.document.annotations[0];
            a.kind = crate::Kind::Pen;
            a.text = None;
            a.points = vec![[100., 100.], [120., 130.], [150., 110.], [190., 140.]];
            a.line_style = Some(capture_core::line_style::Style::for_kind(crate::Kind::Pen));
        }
        if scene == "icons" {
            e.select_only(None);
            e.icon_library.active = true;
        }
        if scene == "capture" {
            e.clip.video_dialog = true;
        }
        let width = if scene == "capture" {
            900.
        } else if scene == "icons" {
            560.
        } else {
            390.
        };
        let mut draw = || {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, if scene == "shadow" { 1200. } else { 900. }),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    if scene == "capture" {
                        e.video_capture_dialog(ui.ctx());
                    } else {
                        e.image_properties_pane(ui);
                    }
                },
            ))
        };
        let mut first = draw();
        for _ in 0..3 {
            let out = draw();
            for (id, delta) in &out.0.textures_delta.set {
                first
                    .0
                    .textures_delta
                    .set
                    .entry(*id)
                    .or_default()
                    .extend(delta.clone());
            }
        }
        let last = draw();
        if let Some(folder) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
            dump_meshes(
                &ctx,
                &folder.to_string_lossy(),
                &format!("refinement-{scene}"),
                first,
                last,
            );
        }
    }
}

#[test]
fn capture_video_dialog_tabs_keep_size_and_icon_toggles_work() {
    use crate::property_layout::tests::PaintedOutput;
    let ctx = egui::Context::default();
    let mut e = super::repair_tests::editor(&ctx);
    e.open_video_capture();
    let draw = |e: &mut Editor, events: Vec<egui::Event>| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100., 900.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| e.video_capture_dialog(ui.ctx()),
        );
        out.textures_delta.clear();
        PaintedOutput(out)
    };
    let find = |out: &PaintedOutput, label: &str| {
        out.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()).center())
            }
            _ => None,
        })
    };
    let area = |ctx: &egui::Context| {
        ctx.memory(|m| {
            m.layer_ids()
                .filter(|l| l.order == egui::Order::Foreground || l.order == egui::Order::Middle)
                .filter_map(|l| m.area_rect(l.id))
                .map(|r| r.size())
                .max_by(|a, b| (a.x * a.y).total_cmp(&(b.x * b.y)))
        })
    };
    draw(&mut e, vec![]);
    let general = draw(&mut e, vec![]);
    let general_size = area(&ctx);
    // General holds capture area, recording, Include and After recording, in that order.
    let y = |label: &str| {
        find(&general, label)
            .unwrap_or_else(|| panic!("{label} drawn"))
            .y
    };
    assert!(
        y("CAPTURE AREA") < y("RECORDING")
            && y("RECORDING") < y("INCLUDE")
            && y("INCLUDE") < y("AFTER RECORDING")
    );
    // Icon toggles flip their settings.
    let mic = e.clip.video_options.as_ref().unwrap().microphone;
    let at = find(&general, "Microphone").unwrap();
    draw(&mut e, super::repair_tests::press(at, true));
    draw(&mut e, super::repair_tests::press(at, false));
    assert_eq!(e.clip.video_options.as_ref().unwrap().microphone, !mic);
    // Step by Step tab: turning it on locks Cursor on and shows the On badge.
    let tab = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("capture-tab", "Step by Step"))))
        .unwrap();
    draw(&mut e, super::repair_tests::press(tab.center(), true));
    draw(&mut e, super::repair_tests::press(tab.center(), false));
    assert!(e.clip.video_steps_tab);
    e.clip.video_options.as_mut().unwrap().instruction.enabled = true;
    e.clip.video_options.as_mut().unwrap().cursor = false;
    let steps = draw(&mut e, vec![]);
    let steps = {
        let _ = steps;
        draw(&mut e, vec![])
    };
    assert!(
        find(&steps, "Move before click").is_some(),
        "lead time shown for direct paths"
    );
    assert!(find(&steps, "Follow the cursor between clicks").is_some());
    assert!(
        e.clip.video_options.as_ref().unwrap().cursor,
        "Step by Step keeps the cursor on"
    );
    assert!(
        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("capture-steps-badge")))
            .is_some()
    );
    // The dialog keeps its size between tabs.
    let steps_size = area(&ctx);
    let (a, b) = (general_size.unwrap(), steps_size.unwrap());
    assert!(
        (a.x - b.x).abs() < 1. && (a.y - b.y).abs() < 1.,
        "{a:?} vs {b:?}"
    );
}
#[test]
fn audio_block_double_click_opens_its_workstation_without_seeking() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.session.document.video.as_mut().unwrap().segments[0]
        .audio
        .push(Audio {
            path: "voice.wav".into(),
            label: "Microphone".into(),
            volume: 1.,
            muted: false,
            fade_in: 0,
            fade_out: 0,
        });
    let draw = |e: &mut Editor, events| {
        let _output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400., 600.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| e.show_video_timeline(ui),
        ));
    };
    draw(&mut e, vec![]);
    draw(&mut e, vec![]);
    let rect = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("media-block", 1u64))))
        .unwrap();
    let at = e.video_time();
    for _ in 0..2 {
        draw(&mut e, press(rect.center(), true));
        draw(&mut e, press(rect.center(), false));
    }
    assert!(e.video.fx.open);
    assert!(!e.video.fx.track_scope);
    assert_eq!(e.video_time(), at);
}

#[test]
fn empty_future_timeline_can_be_scrubbed_without_changing_export_length() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let length = e.session.document.video.as_ref().unwrap().duration();
    e.video_seek(length + 120000);
    assert_eq!(e.video_time(), length + 120000);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().duration(),
        length
    );
}

#[test]
fn shared_dialog_close_target_closes_without_accepting() {
    let ctx = egui::Context::default();
    let mut accepted = false;
    let mut draw = |events| {
        let mut close = false;
        let _output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640., 480.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |_| {
                let response =
                    crate::controls::Modal::new(egui::Id::unique("close-test")).show(&ctx, |ui| {
                        ui.set_width(300.);
                        ui.heading("Save preset");
                        ui.label("Choose a compatible preset or create a new one.");
                        if ui
                            .add(crate::controls::dialog_button("Save", true))
                            .clicked()
                        {
                            accepted = true;
                        }
                    });
                close = response.should_close();
            },
        ));
        close
    };
    draw(vec![]);
    draw(vec![]);
    let rect = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("dialog-close")))
        .unwrap();
    draw(press(rect.center(), true));
    assert!(draw(press(rect.center(), false)));
    assert!(!accepted);
}
