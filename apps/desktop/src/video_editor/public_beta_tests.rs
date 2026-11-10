//! Public screenshots contain only generated demonstration artwork, never a
//! user Library, recordings, preferences, desktop capture or device identifiers.
use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
fn fresh_profile_has_no_projects_or_personal_preferences() {
    if std::env::var_os("CLIPCRAFT_FRESH_PROFILE_PROBE").is_some() {
        let history = crate::history::HistoryPanel::new();
        let prefs = crate::quick_controls::QuickControls::new();
        let presets = crate::presets::Presets::new();
        assert!(history.entries().is_empty());
        assert!(presets.drawing_styles.is_empty());
        assert!(prefs.capture_settings().preview);
        assert_eq!(
            prefs.startup_window(),
            capture_core::quick_preferences::StartupWindow::Editor
        );
        let base =
            std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("ClipCraft");
        assert_eq!(
            history.root().unwrap(),
            base.join("history-v1").canonicalize().unwrap()
        );
        let library = capture_core::history::History::open(&base.join("history-v1")).unwrap();
        assert_eq!(
            library.limits().unwrap(),
            capture_core::history::Limits {
                entries: 1000,
                bytes: 16_000_000_000
            }
        );
        assert_eq!(
            std::fs::read_dir(base.join("history-v1/images"))
                .unwrap()
                .count(),
            0
        );
        println!(
            "Fresh profile: empty Library and presets, Editor startup, opening enabled, maximum capacity."
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","video_editor::public_beta_tests::fresh_profile_has_no_projects_or_personal_preferences","--nocapture"])
        .env("LOCALAPPDATA",root.path()).env("CLIPCRAFT_FRESH_PROFILE_PROBE","1")
        .env_remove("CLIPCRAFT_HISTORY_DIR").env_remove("CLIPCRAFT_STYLE_DIR").env_remove("CLIPCRAFT_PREFERENCES_DIR")
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Fresh profile: empty Library"));
}

fn sample_screen() -> image::RgbaImage {
    sample_variant("Your next great walkthrough", [22, 147, 183, 255])
}

/// The same synthetic workspace with another heading and accent color.
fn sample_variant(heading: &str, accent: [u8; 4]) -> image::RgbaImage {
    let mut d = Document::canvas(960, 540, [241, 245, 249, 255]).unwrap();
    let mut add = |rect: [f32; 4], color: [u8; 4], text: &str, size: f32| {
        let mut a = capture_core::steps::marker(
            d.annotations.len() as u64 + 1,
            [100., 100.],
            40,
            color,
            (1, false),
            [960, 540],
        )
        .unwrap();
        a.start = [rect[0], rect[1]];
        a.end = [rect[0] + rect[2], rect[1] + rect[3]];
        a.width = 1;
        if text.is_empty() {
            a.kind = crate::Kind::Rectangle;
            a.text = None;
            a.fill = Some(color);
            a.corner_radius = 10.;
        } else {
            a.kind = crate::Kind::Text;
            let t = a.text.as_mut().unwrap();
            t.content = text.into();
            t.size_points = size;
            t.horizontal = capture_core::text::HorizontalAlignment::Left;
            t.vertical = capture_core::text::VerticalAlignment::Middle;
        }
        d.annotations.push(a);
    };
    add([0., 0., 960., 54.], [34, 46, 58, 255], "", 0.);
    add(
        [25., 8., 400., 38.],
        [255; 4],
        "FIELDNOTES  /  Team workspace",
        16.,
    );
    add([28., 79., 700., 45.], [30, 50, 67, 255], heading, 27.);
    add(
        [30., 125., 790., 31.],
        [80, 101, 117, 255],
        "Plan clearly. Share context. Help everyone move forward.",
        15.,
    );
    for (x, n, label) in [
        (30., "12", "Published guides"),
        (335., "4", "Ready for review"),
        (640., "28", "Helpful answers"),
    ] {
        add([x, 180., 285., 115.], [255; 4], "", 0.);
        add([x + 18., 192., 210., 51.], accent, n, 30.);
        add([x + 18., 249., 230., 27.], [60, 79, 95, 255], label, 15.);
    }
    add([30., 320., 895., 185.], [255; 4], "", 0.);
    add(
        [49., 332., 600., 35.],
        [35, 54, 70, 255],
        "Project checklist",
        19.,
    );
    for (i, label) in [
        "Prepare your workspace",
        "Capture the essential steps",
        "Review and share with your team",
    ]
    .iter()
    .enumerate()
    {
        add(
            [51., 384. + i as f32 * 35., 22., 22.],
            [28, 157, 186, 255],
            "",
            0.,
        );
        add(
            [86., 380. + i as f32 * 35., 580., 31.],
            [65, 84, 100, 255],
            label,
            15.,
        );
    }
    add([735., 418., 160., 45.], accent, "", 0.);
    add([756., 423., 130., 33.], [255; 4], "Share guide", 15.);
    d.render().unwrap()
}

/// The image guide or video editor used by every public screenshot.
fn showcase(ctx: &egui::Context, video: bool) -> Editor {
    {
        crate::shell::configure(ctx);
        crate::text_layout::configure(ctx);
        let mut e = crate::canvas::tests::editor(ctx);
        let pixels = sample_screen();
        e.shell.backstage = None;
        e.shell.library = false;
        e.shell.properties = true;
        e.history.recent = false;
        e.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
        if video {
            let mut d = super::tests::fixture();
            let p = d.video.as_mut().unwrap();
            p.segments[0].label = "Workspace walkthrough".into();
            let audio = p.add_media_track();
            p.media_kinds
                .insert(audio, capture_core::video::editor::TrackKind::Voiceover);
            let mut segment = capture_core::video::editor::Segment::new(
                2,
                0,
                audio,
                Some(capture_core::video::editor::Source {
                    path: "Narration.wav".into(),
                    size: [1, 1],
                    duration: 12000,
                    kind: capture_core::video::editor::MediaKind::Audio,
                }),
                12000,
            );
            segment.label = "Narration".into();
            segment.fade_in = 350;
            segment.fade_out = 600;
            segment.audio.push(capture_core::video::editor::Audio {
                path: "Narration.wav".into(),
                label: "Narration".into(),
                volume: 1.,
                muted: false,
                fade_in: 0,
                fade_out: 0,
            });
            p.segments.push(segment);
            p.objects.get_mut(&3).unwrap().entrance =
                capture_core::video::editor::Animation::Typewriter;
            p.objects.get_mut(&3).unwrap().exit = capture_core::video::editor::Animation::Fade;
            d.annotations[2].start = [110., 505.];
            d.annotations[2].end = [850., 539.];
            d.annotations[2].text.as_mut().unwrap().content = "Show exactly what matters".into();
            d.annotations[2].text.as_mut().unwrap().size_points = 19.;
            d.annotations[1].start = [716., 388.];
            d.annotations[1].end = [764., 436.];
            e.session = Session::new(d).unwrap();
            e.select_only(Some(3));
            e.tool = None;
            e.video.frame = Some((4500, Arc::new(pixels.clone())));
            e.video.collapsed = false;
            e.video.tab = PropertyTab::Motion;
            e.video.object_tab = PropertyTab::Motion;
            let small = image::imageops::thumbnail(&pixels, 160, 90);
            let tex = ctx.load_texture(
                "public-demo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [small.width() as usize, small.height() as usize],
                    small.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            e.video.thumbnails = (0..16).map(|n| (n * 750, tex.clone())).collect();
            let wave = (0..256)
                .map(|n| {
                    ((n as f32 * 0.17).sin().abs() * 0.65 + 0.1) * ((n as f32 * 0.037).cos().abs())
                })
                .collect();
            e.video
                .media_overviews
                .insert("Narration.wav".into(), (vec![], wave));
        } else {
            let bitmap =
                Arc::new(capture_core::bitmap::Bitmap::transient(Arc::new(pixels)).unwrap());
            let mut d = capture_core::templates::build(
                "three-steps",
                "Share a clearer workflow",
                &[bitmap.clone(), bitmap.clone(), bitmap],
                capture_core::templates::Scale::Fit,
            )
            .unwrap();
            let mut n = 0;
            for a in &mut d.annotations {
                if let Some(t) = &mut a.text {
                    if t.content.starts_with("Step ") {
                        t.content = [
                            "Capture your workspace",
                            "Make the important parts clear",
                            "Share an editable guide",
                        ][n]
                            .into();
                        n = (n + 1) % 3;
                    }
                    if t.content == "Describe the action or key point." {
                        t.content = "Keep every image, label and step editable.".into();
                    }
                }
            }
            e.session = Session::new(d).unwrap();
            e.select_only(None);
            e.choose_tool(
                crate::shell::TOOLS
                    .iter()
                    .position(|t| t.id == capture_core::toolbar::ToolId::Focus)
                    .unwrap(),
            );
        }
        e.needs_render = true;
        e
    }
}

/// Paints the whole window, then `overlay` (a dialog) on top, and saves the
/// meshes for `scripts/render-egui-fixture.py`.
fn paint_window(
    ctx: &egui::Context,
    e: &mut Editor,
    root: &str,
    name: &str,
    overlay: fn(&egui::Context, &mut Editor),
) {
    let mut time = 0.;
    let mut draw = |e: &mut Editor| {
        time += 0.25;
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 960.),
                )),
                focused: true,
                ..Default::default()
            },
            |ui| {
                e.show_shell(ui);
                if e.needs_render {
                    e.refresh_canvas_display(ui.ctx());
                }
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(if e.shell.library {
                                crate::shell::SURROUND
                            } else {
                                crate::shell::CANVAS
                            })
                            .inner_margin(if e.shell.library {
                                egui::Margin {
                                    left: 5,
                                    right: 5,
                                    top: 0,
                                    bottom: 3,
                                }
                            } else {
                                egui::Margin::same(12)
                            }),
                    )
                    .show(ui, |ui| e.show_workspace(ui));
                overlay(ui.ctx(), e);
            },
        ))
    };
    let mut first = draw(e);
    if e.shell.library {
        // Library thumbnails decode on worker threads; wait for every one.
        for _ in 0..200 {
            let frame = draw(e);
            first
                .0
                .textures_delta
                .append(frame.0.textures_delta.clone());
            if e.history.thumbnails_settled() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(e.history.thumbnails_settled(), "Library thumbnails loaded");
    }
    for _ in 0..4 {
        let frame = draw(e);
        first
            .0
            .textures_delta
            .append(frame.0.textures_delta.clone());
    }
    let last = draw(e);
    dump_meshes(ctx, root, name, first, last);
}

#[test]
fn public_image_and_video_workflows_use_synthetic_content() {
    let Ok(root) = std::env::var("CLIPCRAFT_PUBLIC_UI") else {
        return;
    };
    for video in [false, true] {
        let ctx = egui::Context::default();
        let history = tempfile::tempdir().unwrap();
        let mut e = showcase(&ctx, video);
        e.history = crate::history::HistoryPanel::isolated(history.path());
        let name = if video {
            "showcase-video"
        } else {
            "showcase-guide"
        };
        paint_window(&ctx, &mut e, &root, name, |_, _| {});
        assert!(e.history.entries().is_empty());
    }
}

/// The Library, Capture Clip and the Export video privacy review, each with
/// generated demonstration content in an isolated Library.
#[test]
fn public_library_capture_and_privacy_use_synthetic_content() {
    let Ok(root) = std::env::var("CLIPCRAFT_PUBLIC_UI") else {
        return;
    };
    // Library: generated captures in folders, with tags, sources and a pin.
    {
        let ctx = egui::Context::default();
        let history = tempfile::tempdir().unwrap();
        let mut e = showcase(&ctx, false);
        e.history = crate::history::HistoryPanel::isolated(history.path());
        let store = e.history.store.as_mut().unwrap();
        for folder in ["Onboarding", "Product tour", "Support answers"] {
            store.create_folder(folder).unwrap();
        }
        /// Label, accent, folder, tags and website.
        type Item = (
            &'static str,
            [u8; 4],
            &'static str,
            &'static [&'static str],
            Option<&'static str>,
        );
        let items: [Item; 9] = [
            (
                "Welcome checklist",
                [22, 147, 183, 255],
                "Onboarding",
                &["Team"],
                Some("fieldnotes.example"),
            ),
            (
                "Weekly report walkthrough",
                [46, 125, 90, 255],
                "Product tour",
                &["Reports"],
                Some("fieldnotes.example"),
            ),
            (
                "Invite a teammate",
                [124, 82, 196, 255],
                "Onboarding",
                &["Team"],
                None,
            ),
            (
                "Billing settings",
                [196, 112, 38, 255],
                "Support answers",
                &["Billing"],
                Some("billing.example"),
            ),
            (
                "Share a project",
                [22, 147, 183, 255],
                "Product tour",
                &[],
                None,
            ),
            (
                "Notification options",
                [178, 58, 88, 255],
                "Support answers",
                &["Settings"],
                None,
            ),
            (
                "Export a summary",
                [46, 125, 90, 255],
                "Product tour",
                &["Reports"],
                Some("fieldnotes.example"),
            ),
            (
                "Connect a calendar",
                [124, 82, 196, 255],
                "Onboarding",
                &["Settings"],
                None,
            ),
            (
                "Reset a password",
                [196, 112, 38, 255],
                "Support answers",
                &["Settings"],
                Some("help.example"),
            ),
        ];
        let mut ids = vec![];
        for (label, accent, folder, tags, site) in items {
            let pixels = sample_variant(label, accent);
            let d = capture_core::Document::from_image(pixels).unwrap();
            let entry = store
                .create(&d, label, capture_core::history::Origin::Capture)
                .unwrap();
            let tags: Vec<String> = tags.iter().map(|t| (*t).into()).collect();
            let entry = store.edit_metadata(&entry, label, &tags).unwrap();
            store
                .move_to_folder(std::slice::from_ref(&entry.id), Some(folder))
                .unwrap();
            store
                .set_source(&entry.id, Some("Fieldnotes".into()), site.map(Into::into))
                .unwrap();
            ids.push(entry.id);
        }
        store.set_pinned(&ids[0], true).unwrap();
        e.history.refresh();
        e.choose_tool(
            crate::shell::TOOLS
                .iter()
                .position(|t| t.id == capture_core::toolbar::ToolId::Move)
                .unwrap(),
        );
        e.status.clear();
        e.shell.library = true;
        paint_window(&ctx, &mut e, &root, "showcase-library", |_, _| {});
    }
    // Capture Clip opens its settings over the editor.
    {
        let ctx = egui::Context::default();
        let history = tempfile::tempdir().unwrap();
        let mut e = showcase(&ctx, false);
        e.history = crate::history::HistoryPanel::isolated(history.path());
        e.open_clip();
        paint_window(&ctx, &mut e, &root, "showcase-capture", |c, e| {
            e.capture_clip_dialog(c)
        });
        assert!(e.history.entries().is_empty());
    }
    // Export video lists the tracked redaction that still needs review.
    {
        let ctx = egui::Context::default();
        let history = tempfile::tempdir().unwrap();
        let mut e = showcase(&ctx, true);
        e.history = crate::history::HistoryPanel::isolated(history.path());
        super::dialog_gallery_tests::pending_redaction(&mut e);
        e.export_video();
        paint_window(&ctx, &mut e, &root, "showcase-privacy", |c, e| {
            e.show_video_export(c)
        });
        assert!(e.history.entries().is_empty());
    }
}
