use super::*;

fn frame(ctx: &egui::Context, e: &mut Editor) -> crate::property_layout::tests::PaintedOutput {
    crate::property_layout::tests::PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 850.),
            )),
            ..Default::default()
        },
        |ui| e.show_sharing(ui.ctx()),
    ))
}

#[test]
fn share_and_add_a_kit_follow_the_mockup_and_add_items() {
    use crate::color_widgets::tests::text_rect;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.quick_controls
        .set_sensitive_types(vec![capture_core::privacy::custom::CustomType::new(
            "Employee ID",
            "EMP-#####",
        )])
        .unwrap();
    e.open_share_kit();
    frame(&ctx, &mut e);
    let out = frame(&ctx, &mut e);
    for label in [
        "Share a kit",
        "Kit name",
        "INCLUDE",
        "Presets",
        "Blocks",
        "Color palettes",
        "Custom sensitive types",
        "Watermarks",
        "Templates",
        "Save kit…",
    ] {
        assert!(text_rect(&out.shapes, label).is_some(), "{label}");
    }
    // Watermarks and templates start off; sensitive types are on.
    let d = e.sharing.share.as_ref().unwrap();
    assert!(d.include[index(Category::SensitiveTypes)]);
    assert!(!d.include[index(Category::Watermarks)]);

    // A kit with a clashing sensitive type and a new one.
    let mut kit = Kit::new("Support team styles", "ClipDesigner test");
    kit.sensitive_types = vec![
        capture_core::privacy::custom::CustomType::new("employee id", "E-####"),
        capture_core::privacy::custom::CustomType::new("Badge", "B-####"),
    ];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Support team styles.cdkit");
    kit.write(&path).unwrap();
    e.sharing.share = None;
    e.open_kit(&path);
    frame(&ctx, &mut e);
    let out = frame(&ctx, &mut e);
    for label in [
        "Add a kit",
        "Support team styles.cdkit",
        "THIS KIT ADDS",
        "2 custom sensitive types",
        "1 replaces yours",
        "Same names",
        "Keep both",
        "Replace mine",
        "Add 2 items",
        "Nothing is changed until you add it.",
    ] {
        assert!(text_rect(&out.shapes, label).is_some(), "{label}");
    }
    // Keep both numbers the clash; Replace mine takes its place.
    let include = [true; 6];
    e.add_kit(&ctx, &kit, include, false).unwrap();
    let names: Vec<String> = e
        .quick_controls
        .sensitive_types()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["Employee ID", "employee id (2)", "Badge"]);
    e.add_kit(&ctx, &kit, include, true).unwrap();
    let types = e.quick_controls.sensitive_types();
    assert_eq!(types.len(), 3);
    assert_eq!(types[0].pattern, "E-####");
}

#[test]
fn team_kits_add_once_and_again_when_they_change() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    let dir = tempfile::tempdir().unwrap();
    let mut kit = Kit::new("Brand", "ClipDesigner test");
    kit.sensitive_types = vec![capture_core::privacy::custom::CustomType::new(
        "Case", "C-###",
    )];
    kit.write(&dir.path().join("Brand.cdkit")).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not a kit").unwrap();
    let mut team = e.quick_controls.team();
    team.folder = Some(dir.path().to_owned());
    e.quick_controls.set_team(team).unwrap();
    e.sync_team_kits(&ctx);
    assert_eq!(e.quick_controls.sensitive_types().len(), 1);
    assert!(e.quick_controls.team().added.contains_key("Brand.cdkit"));
    // Unchanged: nothing new is added.
    e.sync_team_kits(&ctx);
    assert_eq!(e.quick_controls.sensitive_types().len(), 1);
    // Turned off: an update is not added.
    let mut team = e.quick_controls.team();
    team.off.push("Brand.cdkit".into());
    team.added.clear();
    e.quick_controls.set_team(team).unwrap();
    e.sync_team_kits(&ctx);
    assert_eq!(e.quick_controls.sensitive_types().len(), 1);
}

#[test]
fn a_shared_project_someone_else_has_opens_read_only() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("Shared drives").join("Support");
    std::fs::create_dir_all(&shared).unwrap();
    let project = shared.join("Onboarding guide.iclip");
    capture_core::Document::blank(40, 30)
        .unwrap()
        .save(&project)
        .unwrap();
    assert!(shared_location(&project, None));
    assert!(!shared_location(&dir.path().join("Mine.iclip"), None));
    let sam = Lock {
        name: "Sam".into(),
        computer: "SAM-LAPTOP".into(),
        since_ms: now_ms(),
    };
    assert_eq!(capture_core::lock::take(&project, &sam), Opened::Mine);
    e.open_path(project.clone());
    assert_eq!(e.locked_by_other().map(|l| l.name.as_str()), Some("Sam"));
    assert!(!e.save(false));
    assert!(e.status.contains("Sam is editing"), "{}", e.status);
    // Its Library card notes the shared project and shows who has it.
    e.sharing_tick(&ctx);
    if let Some(id) = e.history.active.clone() {
        let entry = e.history.entry(&id).unwrap();
        assert_eq!(
            entry.organize.shared.as_deref(),
            Some(&*project.to_string_lossy())
        );
        assert_eq!(
            e.shared_holder(&project.to_string_lossy()).as_deref(),
            Some("Sam")
        );
    }
    // The lock on disk is still Sam's.
    assert_eq!(
        capture_core::lock::read(&project, now_ms()),
        Some(sam.clone())
    );
    e.open_lock_copy();
    assert!(e.locked_by_other().is_none() && e.path.is_none());
    // Once Sam closes it, opening it here takes the lock, and closing
    // ClipDesigner gives it back.
    capture_core::lock::release(&project, &sam);
    let mut mine = crate::canvas::tests::editor(&ctx);
    mine.open_path(project.clone());
    assert!(mine.locked_by_other().is_none());
    let held = capture_core::lock::read(&project, now_ms()).unwrap();
    assert_ne!(held.name, "Sam");
    drop(mine);
    assert_eq!(capture_core::lock::read(&project, now_ms()), None);
}
