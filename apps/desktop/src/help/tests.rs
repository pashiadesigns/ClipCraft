use super::*;

fn top(query: &str) -> String {
    let hits = search(query);
    assert!(!hits.is_empty(), "no results for {query:?}");
    articles()[hits[0].index].id.clone()
}
#[test]
fn articles_parse_with_unique_ids_pictures_and_text() {
    let list = articles();
    assert!(list.len() >= 50, "{} articles", list.len());
    let mut ids = std::collections::BTreeSet::new();
    for a in list {
        assert!(ids.insert(a.id.clone()), "duplicate id {}", a.id);
        assert!(!a.title.is_empty() && !a.section.is_empty(), "{}", a.id);
        assert!(!a.blocks.is_empty(), "{} has no text", a.id);
        for key in a.hero.iter().chain(a.blocks.iter().filter_map(|b| match b {
            Block::Art(k, _) => Some(k),
            _ => None,
        })) {
            assert!(has_picture(key), "{}: no picture {key}", a.id);
        }
        if let Some(Block::Pair(l, r, _)) = a.blocks.iter().find(|b| matches!(b, Block::Pair(..))) {
            assert!(art::draw(l).is_some() && art::draw(r).is_some(), "{}", a.id);
        }
    }
    // Articles lead with pictures, not lists alone.
    let pictured = list.iter().filter(|a| a.hero.is_some()).count();
    assert!(
        pictured * 2 >= list.len(),
        "{pictured} of {} have a picture",
        list.len()
    );
}
#[test]
fn search_handles_typos_everyday_words_and_controls() {
    assert_eq!(top("obfuscate"), "obfuscate");
    assert_eq!(top("blurr"), "obfuscate", "typo");
    assert_eq!(top("hide email"), "obfuscate", "everyday words");
    assert_eq!(top("numbered"), "steps");
    assert_eq!(top("gif"), "gif");
    assert_eq!(top("jump"), "jump");
    assert_eq!(top("watermark"), "watermark");
    assert_eq!(top("svg"), "import-image");
    assert_eq!(top("palette"), "colors");
    assert_eq!(top("rename"), "projects");
    assert!(search("zzqqxx").is_empty());
    assert_eq!(
        search("").len(),
        articles().len(),
        "empty search lists everything"
    );
    // Matches are reported for highlighting.
    assert!(search("blurr")[0].matched.iter().any(|w| w == "blur"));
}
#[test]
fn f1_finds_articles_by_control_name() {
    let id = |t: &str| for_control(t).map(|i| articles()[i].id.as_str());
    assert_eq!(id("Obfuscate"), Some("obfuscate"));
    assert_eq!(id("Jump to Object"), Some("jump"));
    assert_eq!(id("Capture Clip"), Some("capture-clip"));
    assert_eq!(
        id("Icons (3)"),
        Some("icons"),
        "counts in names are ignored"
    );
    assert_eq!(id("Some unknown control"), None);
}
#[test]
fn help_window_opens_from_a_request_and_shows_the_article() {
    let ctx = crate::egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    request(&ctx, for_control("Obfuscate"), "");
    let mut shown = String::new();
    for _ in 0..3 {
        let mut out = ctx.run_ui(Default::default(), |ui| e.show_help(ui.ctx()));
        out.textures_delta.clear();
        shown = format!("{:?}", out.shapes);
    }
    assert!(e.shell.help_window.open);
    assert!(e.shell.help, "canvas shortcuts pause while Help is open");
    assert!(shown.contains("Blur and pixelate"));
    assert!(shown.contains("Where to find it"));
}
/// BI7: the article stacks top to bottom and wraps inside the window.
#[test]
fn article_parts_stack_and_wrap_inside_the_window() {
    use crate::egui;
    fn texts(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
            egui::Shape::Text(t) => {
                out.push((t.galley.text().to_owned(), t.visual_bounding_rect()))
            }
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let id = for_control("Obfuscate");
    request(&ctx, id, "");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280., 800.));
    let mut found = vec![];
    for _ in 0..4 {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| e.show_help(ui.ctx()),
        );
        out.textures_delta.clear();
        found.clear();
        for s in &out.shapes {
            texts(&s.shape, &mut found);
        }
    }
    let a = &articles()[id.unwrap()];
    let find = |needle: &str| {
        found
            .iter()
            .find(|(t, _)| t.starts_with(needle))
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{needle:?} not drawn"))
    };
    let title = find(&a.title);
    let strip = find("Where to find it");
    let paragraph = a
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Text(t) => Some(t.chars().take(20).collect::<String>()),
            _ => None,
        })
        .map(|t| find(&t))
        .unwrap();
    assert!(title.bottom() <= strip.top() + 1., "title above the strip");
    assert!(
        strip.bottom() <= paragraph.top() + 1.,
        "strip above the text"
    );
    assert!(
        (paragraph.left() - title.left()).abs() < 40.,
        "text starts under the title, not beside it"
    );
    for (t, r) in found.iter().filter(|(t, _)| !t.trim().is_empty()) {
        assert!(
            r.right() <= screen.right() + 1. && r.height() < 400.,
            "{t:?} runs off the window or wraps to one letter per line: {r:?}"
        );
        assert!(
            t.chars().count() < 4 || r.width() > r.height() * 0.5 || r.height() < 30.,
            "{t:?} is squeezed into a column: {r:?}"
        );
    }
}
#[test]
fn help_pictures_can_be_saved_for_review() {
    let Some(dir) = std::env::var_os("CLIPCRAFT_HELP_ART") else {
        return;
    };
    for key in [
        "blur",
        "redact",
        "arrow",
        "shape",
        "text",
        "callout",
        "steps",
        "penlighter",
        "focus",
        "crop",
        "tilt",
        "watermark",
        "svg",
        "template",
        "palette",
        "timeline",
        "webcam",
    ] {
        art::draw(key)
            .unwrap()
            .save(std::path::Path::new(&dir).join(format!("{key}.png")))
            .unwrap();
    }
}

#[test]
fn every_step_has_a_short_title() {
    let mut count = 0;
    for a in articles() {
        for block in &a.blocks {
            if let Block::Step(text, _, _, title) = block {
                assert!(!title.is_empty(), "{}: {text}", a.id);
                assert!(title.chars().count() <= 26, "{}: {title}", a.id);
                assert!(!title.contains('|'), "{title}");
                count += 1;
            }
        }
    }
    assert!(count >= 130, "{count} steps");
}

fn help_frame(
    ctx: &crate::egui::Context,
    e: &mut crate::Editor,
    size: [f32; 2],
    events: Vec<crate::egui::Event>,
) -> crate::egui::FullOutput {
    let screen = crate::egui::Rect::from_min_size(
        crate::egui::Pos2::ZERO,
        crate::egui::vec2(size[0], size[1]),
    );
    let mut out = ctx.run_ui(
        crate::egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        },
        |ui| e.show_help(ui.ctx()),
    );
    out.textures_delta.clear();
    out
}
fn help_rect(ctx: &crate::egui::Context) -> crate::egui::Rect {
    ctx.data(|d| d.get_temp(crate::egui::Id::unique("help-rect")))
        .unwrap()
}

#[test]
fn the_window_opens_large_and_keeps_its_size_on_every_tab() {
    use crate::egui;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    request(&ctx, for_control("Obfuscate"), "");
    for _ in 0..3 {
        help_frame(&ctx, &mut e, [1600., 1000.], vec![]);
    }
    let first = help_rect(&ctx);
    assert!((first.width() - 1180.).abs() < 2., "{first:?}");
    assert!((first.height() - 760.).abs() < 2., "{first:?}");
    for page in [
        window::Page::Shortcuts,
        window::Page::About,
        window::Page::Article,
    ] {
        e.shell.help_window.set_page(page);
        for _ in 0..3 {
            help_frame(&ctx, &mut e, [1600., 1000.], vec![]);
        }
        let now = help_rect(&ctx);
        assert!(
            (now.min - first.min).length() < 1. && (now.size() - first.size()).length() < 1.,
            "{now:?} {first:?}"
        );
    }
}

#[test]
fn an_article_with_steps_folds_the_list_and_the_strip_brings_it_back() {
    use crate::egui;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    request(&ctx, for_control("Obfuscate"), "");
    for _ in 0..3 {
        help_frame(&ctx, &mut e, [1600., 1000.], vec![]);
    }
    assert!(e.shell.help_window.is_collapsed());
    let strip: egui::Rect = ctx
        .data(|d| d.get_temp(egui::Id::unique("help-strip-rect")))
        .unwrap();
    assert!((strip.width() - 44.).abs() < 1.);
    let tops: Vec<f32> = ctx
        .data(|d| d.get_temp(egui::Id::unique("help-step-tops")))
        .unwrap();
    assert!(tops.windows(2).all(|t| t[1] - t[0] > 60.), "{tops:?}");
    let spot = egui::pos2(strip.center().x, strip.bottom() - 30.);
    help_frame(
        &ctx,
        &mut e,
        [1600., 1000.],
        vec![egui::Event::PointerMoved(spot)],
    );
    for pressed in [true, false] {
        help_frame(
            &ctx,
            &mut e,
            [1600., 1000.],
            vec![egui::Event::PointerButton {
                pos: spot,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
        );
    }
    assert!(
        !e.shell.help_window.is_collapsed(),
        "clicking anywhere on the strip opens it"
    );
}

#[test]
fn an_article_without_steps_keeps_the_list_open() {
    let index = articles()
        .iter()
        .position(|a| !a.has_steps())
        .expect("an overview article");
    let mut state = window::State::default();
    state.open_article(index);
    assert!(!state.is_collapsed());
}

#[test]
fn every_default_shortcut_appears_in_exactly_one_group() {
    use capture_core::editor_keybinds::{Function, TOOL_DEFAULTS};
    let keys = capture_core::keybinds::Keybinds::default();
    let groups = window::shortcut_groups(&keys);
    assert_eq!(
        groups.iter().map(|g| g.0).collect::<Vec<_>>(),
        ["File", "Edit", "Tools", "Arrange", "View", "Video"]
    );
    let rows: Vec<&str> = groups
        .iter()
        .flat_map(|g| g.2.iter().map(|r| r.0.as_str()))
        .collect();
    let mut unique = rows.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), rows.len(), "{rows:?}");
    assert_eq!(rows.len(), Function::ALL.len() + TOOL_DEFAULTS.len() + 2);
    assert_eq!(window::key_caps("Ctrl+Shift+S"), ["Ctrl", "Shift", "S"]);
    assert_eq!(window::key_caps("Ctrl+="), ["Ctrl", "="]);
}
