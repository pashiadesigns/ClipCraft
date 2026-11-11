//! Dialog audit gallery (BJ): opens every dialog on its own, headless, and
//! saves its painted meshes so `scripts/render-egui-fixture.py` can turn them
//! into screenshots. Set CLIPDESIGNER_DIALOG_AUDIT_DIR to write them; without it
//! the test only checks that every dialog opens and draws.
use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

type Open = fn(&egui::Context, &mut Editor);
pub(super) type Show = fn(&egui::Context, &mut Editor);

fn image_editor(ctx: &egui::Context) -> Editor {
    crate::shell::configure(ctx);
    crate::text_layout::configure(ctx);
    crate::canvas::tests::editor(ctx)
}

pub(super) fn dialogs() -> Vec<(&'static str, bool, Open, Show)> {
    vec![
        (
            "capture-clip",
            false,
            |_, e| e.open_clip(),
            |c, e| e.capture_clip_dialog(c),
        ),
        (
            "capture-video",
            false,
            |_, e| e.clip.video_dialog = true,
            |c, e| e.video_capture_dialog(c),
        ),
        (
            "export-image",
            false,
            |_, e| {
                e.export_preferences.watermark_ready = true;
                e.export_preferences.options.watermark = Some(Default::default());
                e.begin_export(false);
            },
            |c, e| e.show_export(c, None),
        ),
        (
            "print",
            false,
            |_, e| e.begin_export(true),
            |c, e| e.show_export(c, None),
        ),
        (
            "new-image",
            false,
            |_, e| e.new_image = Some(crate::new_image::NewImage::default()),
            |c, e| e.show_new_image(c),
        ),
        (
            "resize-image",
            false,
            |_, e| e.begin_resize(),
            |c, e| e.show_resize(c),
        ),
        (
            "resize-canvas",
            false,
            |_, e| e.begin_resize_canvas(),
            |c, e| e.show_resize_canvas(c),
        ),
        (
            "page-setup",
            false,
            |c, e| e.open_page_setup(c),
            |c, e| e.show_page_setup(c),
        ),
        (
            "save-template",
            false,
            |_, e| e.begin_save_template(),
            |c, e| e.show_template_picker(c),
        ),
        (
            "blocks",
            false,
            |_, e| e.presets.reusable.open_groups(None),
            |c, e| e.presets.reusable.show(c),
        ),
        (
            "help",
            false,
            |c, _| crate::help::request(c, None, ""),
            |c, e| e.show_help(c),
        ),
        (
            "export-video",
            true,
            |_, e| {
                e.export_preferences.watermark_ready = true;
                e.export_preferences.options.watermark = Some(Default::default());
                pending_redaction(e);
                e.export_video();
            },
            |c, e| e.show_video_export(c),
        ),
        (
            "gif-studio",
            true,
            |_, e| e.open_gif_studio(),
            |c, e| e.show_gif_studio(c),
        ),
        (
            "new-video",
            true,
            |_, e| e.new_video(capture_core::video::templates::Template::ALL[0]),
            |c, e| e.show_new_video(c),
        ),
        (
            "find-sensitive-video",
            true,
            |_, e| e.open_video_privacy_review(),
            |c, e| e.show_video_privacy_review(c),
        ),
        (
            "find-sensitive-video-results",
            true,
            |_, e| {
                e.open_video_privacy_review();
                super::privacy::seed_results(e);
            },
            |c, e| e.show_video_privacy_review(c),
        ),
        (
            "find-sensitive-image-results",
            false,
            |_, e| {
                e.open_privacy_review();
                crate::privacy_review::seed_results(e);
            },
            |c, e| e.show_privacy_review(c),
        ),
        (
            "find-sensitive-image",
            false,
            |_, e| e.open_privacy_review(),
            |c, e| e.show_privacy_review(c),
        ),
        (
            "narration",
            true,
            |_, e| e.open_voiceover(),
            |c, e| e.show_voiceover_recorder(c),
        ),
    ]
}

/// One tracked redaction that still needs review, with a held range.
pub(super) fn pending_redaction(e: &mut Editor) {
    use capture_core::video::motion_path::{Node, Path, Purpose};
    let mut p = e.session.document.video.clone().unwrap();
    let node = |at: u64, confidence: f32| Node {
        at,
        rect: [100., 100., 80., 20.],
        visible: true,
        sprite: 0,
        confidence,
    };
    if let Some(t) = p.objects.values_mut().next() {
        t.label = "Email \u{b7} sample".into();
        t.motion_path = Some(Arc::new(Path {
            purpose: Purpose::Redaction,
            reference: [100., 100., 80., 20.],
            nodes: vec![node(0, 1.), node(600, 0.), node(900, 1.)],
            sprites: vec![],
            smooth: false,
            enabled: true,
            offset_ms: 0,
            reviewed: false,
            reduction: Default::default(),
            inputs: vec![],
            recorded: None,
        }));
    }
    e.session.document.video = Some(p);
}
fn frame(ctx: &egui::Context, e: &mut Editor, show: Show, time: f64) -> PaintedOutput {
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            focused: true,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            ..Default::default()
        },
        |ui| show(ui.ctx(), e),
    ))
}

#[test]
fn every_dialog_opens_and_draws_for_the_audit() {
    let root = std::env::var_os("CLIPDESIGNER_DIALOG_AUDIT_DIR");
    let mut failed = vec![];
    for (name, video, open, show) in dialogs() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let ctx = egui::Context::default();
            let mut e = if video {
                super::repair_tests::editor(&ctx)
            } else {
                image_editor(&ctx)
            };
            open(&ctx, &mut e);
            let mut first = frame(&ctx, &mut e, show, 0.);
            for i in 1..4 {
                let mut next = frame(&ctx, &mut e, show, i as f64 * 0.25);
                if i < 3 {
                    next.0.textures_delta.clear();
                } else {
                    if let Some(root) = &root {
                        dump_meshes(&ctx, &root.to_string_lossy(), name, first, next);
                    }
                    return;
                }
                first
                    .0
                    .textures_delta
                    .set
                    .extend(std::mem::take(&mut next.0.textures_delta.set));
            }
        }));
        if result.is_err() {
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "dialogs that failed to draw: {failed:?}");
}
