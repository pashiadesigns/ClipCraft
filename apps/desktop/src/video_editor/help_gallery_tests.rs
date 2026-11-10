//! Help pictures: renders each scene from the real app with a made-up sample
//! project and saves it for `scripts/render-help-pictures.py`. Set
//! CLIPCRAFT_HELP_PICTURES_DIR to write them; without it the test checks that
//! the scenes draw and that every picture an article names exists.
use super::dialog_gallery_tests::{Show, dialogs};
use super::*;
use crate::backstage::Page;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use crate::ribbon::RibbonTab;
use capture_core::toolbar::ToolId;

#[derive(Clone, Copy)]
enum Crop {
    Whole,
    Ribbon,
    Tabs,
    Canvas,
    Pane,
    Timeline,
    Modal,
    /// A dialog without its last rows (the sample has no real media).
    ModalTop(f32),
    /// The first ring, with this much room around it.
    Around(f32, f32),
    /// The properties pane around the first ring: room above and below it.
    PaneAround(f32, f32),
}
#[derive(Clone, Copy)]
enum Ring {
    /// A control's visible text.
    T(&'static str),
    /// A ribbon button: the icon above its label too.
    Up(&'static str),
    /// The first object on the canvas.
    Object,
}
type Setup = Box<dyn Fn(&egui::Context, &mut Editor)>;
/// Wide enough that the whole Insert ribbon shows without overflow.
const WIDTH: f32 = 1440.;
const HEIGHT: f32 = 810.;
/// Tall enough that every group in the properties pane shows without scrolling.
const TALL: f32 = 1500.;
struct Shot {
    key: String,
    video: bool,
    setup: Setup,
    show: Option<Show>,
    crop: Crop,
    rings: Vec<Ring>,
}
fn tile(text: &str, r: [f32; 4], size: f32, color: [u8; 4], id: u64) -> crate::Annotation {
    let mut a =
        capture_core::steps::marker(id, [20., 20.], 32, color, (1, false), [960, 540]).unwrap();
    a.kind = crate::Kind::Text;
    a.step_outline = None;
    a.start = [r[0], r[1]];
    a.end = [r[0] + r[2], r[1] + r[3]];
    let t = a.text.as_mut().unwrap();
    t.content = text.into();
    t.size_points = size;
    t.bold = false;
    t.color = Some(color);
    t.horizontal = capture_core::text::HorizontalAlignment::Left;
    a
}
/// A made-up account page. Nothing in it comes from a real project.
fn sample_image() -> image::RgbaImage {
    let mut d = capture_core::Document::blank(960, 540).unwrap();
    let ink = [38, 48, 54, 255];
    let soft = [104, 118, 126, 255];
    let mut bar =
        capture_core::steps::marker(1, [20., 20.], 32, ink, (1, false), [960, 540]).unwrap();
    bar.kind = crate::Kind::Rectangle;
    bar.text = None;
    bar.step_outline = None;
    bar.start = [0., 0.];
    bar.end = [960., 56.];
    bar.width = 1;
    bar.color = [24, 88, 112, 255];
    bar.fill = Some([24, 88, 112, 255]);
    let mut side = bar.clone();
    side.id = 2;
    side.end = [190., 540.];
    side.start = [0., 56.];
    side.color = [226, 233, 236, 255];
    side.fill = Some([226, 233, 236, 255]);
    d.annotations = vec![
        bar,
        side,
        tile(
            "Northwind Support",
            [24., 12., 400., 32.],
            22.,
            [255, 255, 255, 255],
            3,
        ),
        tile("Overview", [24., 84., 150., 26.], 15., ink, 4),
        tile("Tickets", [24., 122., 150., 26.], 15., ink, 5),
        tile("Billing", [24., 160., 150., 26.], 15., ink, 6),
        tile("Settings", [24., 198., 150., 26.], 15., ink, 7),
        tile("Account details", [230., 84., 600., 44.], 28., ink, 8),
        tile("Contact email", [230., 156., 180., 26.], 15., soft, 9),
        tile("alex@example.com", [430., 156., 300., 26.], 15., ink, 10),
        tile("Phone", [230., 196., 180., 26.], 15., soft, 11),
        tile("(555) 010-0143", [430., 196., 300., 26.], 15., ink, 12),
        tile("Plan", [230., 236., 180., 26.], 15., soft, 13),
        tile("Team, 12 seats", [430., 236., 300., 26.], 15., ink, 14),
        tile("Recent activity", [230., 300., 400., 34.], 21., ink, 15),
        tile(
            "Invoice 1042 was paid on 4 March",
            [230., 346., 620., 26.],
            15.,
            soft,
            16,
        ),
        tile(
            "Two new teammates accepted their invites",
            [230., 384., 620., 26.],
            15.,
            soft,
            17,
        ),
        tile(
            "Password policy was updated",
            [230., 422., 620., 26.],
            15.,
            soft,
            18,
        ),
    ];
    d.promote_features();
    d.render().unwrap()
}
#[derive(Clone, Copy, PartialEq)]
enum Obj {
    None,
    Arrow,
    Text,
    Rect,
    Callout,
    Steps,
    Pen,
    Blur,
    Pixelate,
    Redact,
    Focus,
    Two,
    Magnify,
    Image,
    ImageTilted,
    Icon,
    Eraser,
}
const EMAIL: ([f32; 2], [f32; 2]) = ([424., 150.], [640., 188.]);
fn objects(e: &mut Editor, what: Obj) -> Vec<crate::Annotation> {
    use crate::Kind as K;
    let (a, b) = EMAIL;
    let accent = [228, 70, 60, 255];
    let one = |kind, a, b| e.new_annotation(kind, a, b);
    match what {
        Obj::None => vec![],
        Obj::Arrow => {
            let mut x = one(K::Arrow, [760., 330.], [650., 200.]);
            x.color = accent;
            x.width = 6;
            vec![x]
        }
        Obj::Text => {
            let mut x = one(K::Text, [640., 250.], [900., 300.]);
            x.text = Some(capture_core::text::TextStyle {
                content: "Update this email".into(),
                size_points: 22.,
                bold: true,
                color: Some(accent),
                ..Default::default()
            });
            vec![x]
        }
        Obj::Rect => {
            let mut x = one(K::Rectangle, [418., 146.], [650., 192.]);
            x.color = accent;
            x.width = 4;
            x.corner_radius = 8.;
            vec![x]
        }
        Obj::Callout => {
            let (start, end) = ([650., 240.], [900., 310.]);
            let mut x = one(K::Callout, start, end);
            let mut style = capture_core::callout::CalloutStyle::for_box(start, end, [960, 540]);
            style.fill = [255, 255, 255, 255];
            style.outline = [24, 145, 193, 255];
            style.pointer = [560., 190.];
            x.callout = Some(style);
            x.text = Some(capture_core::text::TextStyle {
                content: "Check this address".into(),
                size_points: 18.,
                bold: true,
                color: Some([28, 36, 40, 255]),
                horizontal: capture_core::text::HorizontalAlignment::Center,
                vertical: capture_core::text::VerticalAlignment::Middle,
                ..Default::default()
            });
            vec![x]
        }
        Obj::Steps => [[400., 169.], [400., 209.], [400., 249.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                capture_core::steps::marker(
                    100 + i as u64,
                    p,
                    32,
                    [24, 145, 193, 255],
                    (i as u32 + 1, false),
                    [960, 540],
                )
                .unwrap()
            })
            .collect(),
        Obj::Pen => {
            let mut x = one(K::Highlighter, [426., 232.], [590., 266.]);
            x.width = 1;
            x.highlighter_rect = true;
            x.color = [255, 222, 28, 120];
            vec![x]
        }
        Obj::Blur => vec![one(K::Blur, a, b)],
        Obj::Pixelate => vec![one(K::Pixelate, a, b)],
        Obj::Redact => vec![one(K::Redact, a, b)],
        Obj::Focus => {
            let mut x = one(K::Rectangle, [418., 146.], [760., 266.]);
            x.focus = Some(capture_core::focus::Focus::default());
            vec![x]
        }
        Obj::Two => {
            let mut x = one(K::Rectangle, [650., 300.], [760., 370.]);
            x.color = accent;
            x.width = 4;
            let mut y = one(K::Ellipse, [790., 340.], [900., 440.]);
            y.id = x.id + 1;
            y.color = [24, 145, 193, 255];
            y.width = 4;
            vec![x, y]
        }
        Obj::Magnify => {
            let mut x = one(K::Rectangle, [424., 150.], [600., 190.]);
            x.color = [24, 145, 193, 255];
            x.width = 3;
            x.focus = Some(capture_core::focus::Focus {
                darken: 0.,
                magnify: Some(Default::default()),
                ..Default::default()
            });
            vec![x]
        }
        Obj::Image | Obj::ImageTilted => {
            let picture = image::imageops::thumbnail(&sample_image(), 300, 169);
            let d = capture_core::Document::from_image(picture).unwrap();
            let raster = capture_core::RasterImage {
                width: d.width,
                height: d.height,
                png: d.source_png.clone(),
            };
            let mut bitmap = capture_core::bitmap::Bitmap::new(Arc::new(raster)).unwrap();
            if what == Obj::ImageTilted {
                bitmap = bitmap
                    .with_perspective(Some(
                        capture_core::bitmap::Perspective::TiltLeft.quad(0.6, 0.5),
                    ))
                    .unwrap();
            }
            let id = one(K::Rectangle, [0., 0.], [10., 10.]).id;
            let mut x = Arc::new(bitmap).annotation(id);
            x.start = [620., 300.];
            x.end = [920., 469.];
            vec![x]
        }
        Obj::Icon => {
            let bitmap = capture_core::bitmap::Bitmap::from_vector(
                capture_core::icon_library::vector::VectorIcon {
                    colors: vec![[228, 70, 60, 255]],
                    paths: vec![capture_core::icon_library::vector::Path {
                        data: "M12 2l3 7h7l-6 5 2 8-6-4-6 4 2-8-6-5h7z".into(),
                        color: 0,
                        stroke: None,
                    }],
                },
            )
            .unwrap()
            .into_icon()
            .named_icon("Star");
            let id = one(K::Rectangle, [0., 0.], [10., 10.]).id;
            let mut x = Arc::new(bitmap).annotation(id);
            x.start = [660., 240.];
            x.end = [760., 340.];
            vec![x]
        }
        Obj::Eraser => {
            let mut x = one(K::Eraser, [430., 240.], [620., 250.]);
            x.points = (0..24).map(|i| [430. + i as f32 * 8., 248.]).collect();
            x.width = 18;
            vec![x]
        }
    }
}
fn tool_index(tool: ToolId) -> usize {
    crate::shell::TOOLS
        .iter()
        .position(|t| t.id == tool)
        .unwrap()
}
/// The sample image with objects on it, on a ribbon tab.
fn image_setup(tab: RibbonTab, what: Obj, tool: Option<ToolId>, select: bool) -> Setup {
    Box::new(move |_, e| {
        let mut d = capture_core::Document::from_image(sample_image()).unwrap();
        let objs = objects(e, what);
        d.annotations = objs;
        d.promote_features();
        e.session = capture_core::Session::new(d).unwrap();
        e.shell.ribbon_tab = tab;
        e.select_only(None);
        if let Some(tool) = tool {
            e.choose_tool(tool_index(tool));
        } else {
            e.tool = None;
        }
        if select && let Some(id) = e.session.document.annotations.first().map(|a| a.id) {
            e.select_only(Some(id));
            e.tool = None;
        }
        e.needs_render = true;
    })
}
fn video_setup(select: Option<usize>) -> Setup {
    Box::new(move |_, e| {
        let source = Arc::new(sample_image());
        let bitmap = Arc::new(capture_core::bitmap::Bitmap::transient(source).unwrap());
        // Keep the fixture's objects clear of the sample page's text.
        for a in &mut e.session.document.annotations {
            if a.kind == crate::Kind::Text {
                a.start = [640., 260.];
                a.end = [930., 310.];
                if let Some(t) = &mut a.text {
                    t.size_points = 20.;
                }
            } else if let Ok(moved) = a.translated([560., 180.], [960, 540]) {
                *a = moved;
            }
        }
        e.video.frame = Some((4500, Arc::new(image::RgbaImage::new(1, 1))));
        e.video.frame_layers = vec![(1, bitmap)];
        if let Some(i) = select {
            e.video.selected = Selection::Recording(i);
        }
    })
}
fn dialog(name: &'static str) -> (bool, Setup, Show) {
    let (_, video, open, show) = dialogs()
        .into_iter()
        .find(|d| d.0 == name)
        .unwrap_or_else(|| panic!("no dialog {name}"));
    (video, Box::new(open), show)
}
fn shots() -> Vec<Shot> {
    use Crop as C;
    use Obj as O;
    use Ring as R;
    let mut v: Vec<Shot> = vec![];
    fn add(
        v: &mut Vec<Shot>,
        key: impl Into<String>,
        video: bool,
        setup: Setup,
        show: Option<Show>,
        crop: Crop,
        rings: &[Ring],
    ) {
        v.push(Shot {
            key: key.into(),
            video,
            setup,
            show,
            crop,
            rings: rings.to_vec(),
        });
    }
    let image = |v: &mut Vec<Shot>, key: &str, tab, what, tool, select, crop, rings: &[Ring]| {
        add(
            v,
            key,
            false,
            image_setup(tab, what, tool, select),
            None,
            crop,
            rings,
        );
    };
    let dlg = |v: &mut Vec<Shot>, key: &str, name: &'static str, rings: &[Ring]| {
        let (video, setup, show) = dialog(name);
        add(v, key, video, setup, Some(show), C::Modal, rings);
    };
    let page = |v: &mut Vec<Shot>, key: &str, p: Page, rings: &[Ring]| {
        let base = image_setup(RibbonTab::Insert, O::None, None, false);
        let setup: Setup = Box::new(move |c, e| {
            base(c, e);
            e.shell.backstage = Some(p);
        });
        add(v, key, false, setup, None, C::Whole, rings);
    };
    let vid = |v: &mut Vec<Shot>, key: &str, select, crop, rings: &[Ring]| {
        add(v, key, true, video_setup(select), None, crop, rings);
    };
    use RibbonTab::{Export, Formatting, Insert};
    // One article per tool: the button, the object, its settings, the result.
    let tool = |v: &mut Vec<Shot>, id: &str, t, label: &'static str, what, pane: &'static str| {
        image(
            v,
            &format!("{id}-1"),
            Insert,
            O::None,
            None,
            false,
            C::Ribbon,
            &[R::Up(label)],
        );
        image(
            v,
            &format!("{id}-2"),
            Insert,
            what,
            None,
            true,
            C::Canvas,
            &[R::Object],
        );
        image(
            v,
            &format!("{id}-3"),
            Insert,
            what,
            None,
            true,
            C::Pane,
            &[R::T(pane)],
        );
        image(
            v,
            &format!("{id}-hero"),
            Insert,
            what,
            None,
            false,
            C::Canvas,
            &[],
        );
        let _ = t;
    };
    tool(&mut v, "lines", ToolId::Lines, "Lines", O::Arrow, "Style");
    tool(&mut v, "text", ToolId::Text, "Text", O::Text, "Text");
    tool(&mut v, "shapes", ToolId::Shape, "Shape", O::Rect, "Style");
    tool(
        &mut v,
        "callouts",
        ToolId::Shape,
        "Shape",
        O::Callout,
        "Text",
    );
    tool(&mut v, "steps", ToolId::Steps, "Steps", O::Steps, "Style");
    tool(
        &mut v,
        "penlighter",
        ToolId::Penlighter,
        "Penlighter",
        O::Pen,
        "Presets",
    );
    tool(&mut v, "focus", ToolId::Focus, "Focus", O::Focus, "Focus");
    tool(
        &mut v,
        "magnify",
        ToolId::MagnifiedCallout,
        "Magnify",
        O::Focus,
        "Presets",
    );
    tool(&mut v, "icons", ToolId::Icons, "Icons", O::Rect, "Style");
    tool(&mut v, "move", ToolId::Move, "Move", O::Rect, "Style");
    tool(
        &mut v,
        "redact",
        ToolId::Obfuscate,
        "Obfuscate",
        O::Redact,
        "Presets",
    );
    image(
        &mut v,
        "obfuscate-1",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::Up("Obfuscate")],
    );
    image(
        &mut v,
        "obfuscate-2",
        Insert,
        O::Blur,
        None,
        true,
        C::Pane,
        &[R::T("Presets")],
    );
    image(
        &mut v,
        "obfuscate-3",
        Insert,
        O::Blur,
        None,
        true,
        C::Pane,
        &[R::T("Intensity")],
    );
    image(
        &mut v,
        "obfuscate-hero",
        Insert,
        O::Pixelate,
        None,
        false,
        C::Canvas,
        &[],
    );
    image(
        &mut v,
        "eraser-1",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::Up("Eraser")],
    );
    image(
        &mut v,
        "eraser-2",
        Insert,
        O::None,
        Some(ToolId::Eraser),
        false,
        C::Canvas,
        &[],
    );
    image(
        &mut v,
        "eraser-hero",
        Insert,
        O::None,
        Some(ToolId::Eraser),
        false,
        C::Whole,
        &[],
    );
    image(
        &mut v,
        "cutout-1",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::Up("Cut Out")],
    );
    image(
        &mut v,
        "cutout-2",
        Insert,
        O::None,
        Some(ToolId::CutOut),
        false,
        C::Canvas,
        &[],
    );
    image(
        &mut v,
        "cutout-hero",
        Insert,
        O::None,
        Some(ToolId::CutOut),
        false,
        C::Whole,
        &[],
    );
    // Crop opens its own dock.
    let crop_setup = |tab| -> Setup {
        let base = image_setup(tab, O::None, None, false);
        Box::new(move |c, e| {
            base(c, e);
            e.begin_crop();
            e.crop.as_mut().unwrap().bounds = [120, 60, 860, 480];
        })
    };
    add(
        &mut v,
        "crop-1",
        false,
        image_setup(Insert, O::None, None, false),
        None,
        C::Ribbon,
        &[R::Up("Crop")],
    );
    add(
        &mut v,
        "crop-2",
        false,
        crop_setup(Insert),
        None,
        C::Canvas,
        &[],
    );
    add(
        &mut v,
        "crop-3",
        false,
        crop_setup(Insert),
        None,
        C::Around(300., 90.),
        &[R::T("Apply crop")],
    );
    add(
        &mut v,
        "crop-hero",
        false,
        crop_setup(Insert),
        None,
        C::Whole,
        &[],
    );
    // Getting started.
    image(
        &mut v,
        "welcome-1",
        Insert,
        O::None,
        None,
        false,
        C::Tabs,
        &[R::T("Capture Clip"), R::T("Capture Video")],
    );
    image(
        &mut v,
        "welcome-2",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::Up("Shape")],
    );
    image(
        &mut v,
        "welcome-3",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Style")],
    );
    image(
        &mut v,
        "welcome-4",
        Export,
        O::None,
        None,
        false,
        C::Tabs,
        &[R::T("Export")],
    );
    image(
        &mut v,
        "welcome-hero",
        Insert,
        O::Callout,
        None,
        true,
        C::Whole,
        &[],
    );
    image(
        &mut v,
        "capture-clip-1",
        Insert,
        O::None,
        None,
        false,
        C::Tabs,
        &[R::T("Capture Clip")],
    );
    dlg(
        &mut v,
        "capture-clip-2",
        "capture-clip",
        &[R::T("Region"), R::T("Capture")],
    );
    image(
        &mut v,
        "capture-clip-3",
        Insert,
        O::Focus,
        None,
        false,
        C::Canvas,
        &[R::Object],
    );
    image(
        &mut v,
        "capture-clip-hero",
        Insert,
        O::Rect,
        None,
        false,
        C::Whole,
        &[],
    );
    image(
        &mut v,
        "capture-video-1",
        Insert,
        O::None,
        None,
        false,
        C::Tabs,
        &[R::T("Capture Video")],
    );
    dlg(
        &mut v,
        "capture-video-2",
        "capture-video",
        &[R::T("Record")],
    );
    vid(&mut v, "capture-video-hero", None, C::Whole, &[]);
    dlg(
        &mut v,
        "step-by-step-video-1",
        "capture-video",
        &[R::T("Step by Step")],
    );
    vid(&mut v, "step-by-step-video-3", None, C::Timeline, &[]);
    vid(&mut v, "step-by-step-video-hero", None, C::Timeline, &[]);
    dlg(&mut v, "webcam-1", "capture-video", &[R::T("Webcam")]);
    vid(&mut v, "webcam-3", Some(0), C::Canvas, &[]);
    vid(&mut v, "webcam-hero", None, C::Canvas, &[]);
    image(
        &mut v,
        "editor-hero",
        Insert,
        O::Rect,
        None,
        true,
        C::Whole,
        &[],
    );
    // Privacy.
    image(
        &mut v,
        "sensitive-1",
        Insert,
        O::Redact,
        None,
        true,
        C::Pane,
        &[R::T("Find Sensitive Info…")],
    );
    let sensitive = || -> Setup {
        let base = image_setup(Insert, O::None, None, false);
        Box::new(move |c, e| {
            base(c, e);
            e.open_privacy_review();
            crate::privacy_review::seed_results(e);
        })
    };
    let (_, _, sensitive_show) = dialog("find-sensitive-image-results");
    add(
        &mut v,
        "sensitive-2",
        false,
        sensitive(),
        Some(sensitive_show),
        C::Modal,
        &[R::T("Scan again")],
    );
    add(
        &mut v,
        "sensitive-3",
        false,
        sensitive(),
        Some(sensitive_show),
        C::Modal,
        &[R::T("COVER WITH")],
    );
    dlg(
        &mut v,
        "sensitive-4",
        "export-video",
        &[R::T("Mark all as reviewed…")],
    );
    add(
        &mut v,
        "sensitive-hero",
        false,
        sensitive(),
        Some(sensitive_show),
        C::Modal,
        &[],
    );
    // Images.
    image(
        &mut v,
        "import-image-1",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::Up("Paste")],
    );
    image(
        &mut v,
        "import-image-3",
        Insert,
        O::Rect,
        None,
        true,
        C::Canvas,
        &[R::Object],
    );
    image(
        &mut v,
        "import-image-hero",
        Insert,
        O::Rect,
        None,
        true,
        C::Canvas,
        &[],
    );
    image(
        &mut v,
        "image-objects-1",
        Insert,
        O::Rect,
        None,
        true,
        C::Canvas,
        &[R::Object],
    );
    image(
        &mut v,
        "image-objects-2",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Style")],
    );
    image(
        &mut v,
        "image-objects-3",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Style")],
    );
    image(
        &mut v,
        "image-objects-hero",
        Insert,
        O::Rect,
        None,
        true,
        C::Whole,
        &[],
    );
    image(
        &mut v,
        "perspective-1",
        Insert,
        O::Rect,
        None,
        true,
        C::Canvas,
        &[R::Object],
    );
    image(
        &mut v,
        "perspective-2",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Style")],
    );
    for key in ["perspective-3", "perspective-hero"] {
        image(&mut v, key, Insert, O::Rect, None, true, C::Pane, &[]);
    }
    // Formatting.
    image(
        &mut v,
        "colors-1",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Style")],
    );
    image(
        &mut v,
        "colors-hero",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[],
    );
    image(
        &mut v,
        "properties-hero",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Style")],
    );
    image(
        &mut v,
        "presets-1",
        Insert,
        O::Rect,
        None,
        true,
        C::Canvas,
        &[R::Object],
    );
    image(
        &mut v,
        "presets-2",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Presets")],
    );
    image(
        &mut v,
        "presets-3",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[R::T("Presets")],
    );
    image(
        &mut v,
        "presets-hero",
        Insert,
        O::Rect,
        None,
        true,
        C::Pane,
        &[],
    );
    image(
        &mut v,
        "arrange-1",
        Formatting,
        O::Two,
        None,
        true,
        C::Canvas,
        &[R::Object],
    );
    image(
        &mut v,
        "arrange-2",
        Formatting,
        O::Two,
        None,
        true,
        C::Ribbon,
        &[R::T("Arrange")],
    );
    image(
        &mut v,
        "arrange-hero",
        Formatting,
        O::Two,
        None,
        false,
        C::Canvas,
        &[],
    );
    // Advanced.
    image(
        &mut v,
        "layers-1",
        Formatting,
        O::Two,
        None,
        false,
        C::Ribbon,
        &[R::T("Grouping")],
    );
    image(
        &mut v,
        "layers-hero",
        Formatting,
        O::Two,
        None,
        false,
        C::Whole,
        &[],
    );
    image(
        &mut v,
        "resize-1",
        Formatting,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::T("Size")],
    );
    dlg(&mut v, "resize-2", "resize-image", &[R::T("Apply")]);
    dlg(&mut v, "resize-hero", "resize-image", &[]);
    image(
        &mut v,
        "snapping-1",
        Formatting,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::T("Guides")],
    );
    image(
        &mut v,
        "snapping-2",
        Formatting,
        O::Two,
        None,
        true,
        C::Canvas,
        &[R::Object],
    );
    image(
        &mut v,
        "snapping-hero",
        Formatting,
        O::Two,
        None,
        true,
        C::Canvas,
        &[],
    );
    image(
        &mut v,
        "grab-text-1",
        Formatting,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::Up("Grab Text")],
    );
    image(
        &mut v,
        "grab-text-hero",
        Formatting,
        O::None,
        None,
        false,
        C::Canvas,
        &[],
    );
    // Export.
    image(
        &mut v,
        "export-1",
        Export,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::T("Export")],
    );
    dlg(&mut v, "export-2", "export-image", &[]);
    dlg(&mut v, "export-hero", "export-image", &[]);
    dlg(&mut v, "watermark-1", "export-image", &[R::T("WATERMARK")]);
    dlg(&mut v, "watermark-2", "export-image", &[R::T("WATERMARK")]);
    dlg(&mut v, "watermark-hero", "export-image", &[]);
    image(
        &mut v,
        "print-1",
        Export,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::Up("Print")],
    );
    dlg(&mut v, "print-2", "print", &[]);
    dlg(&mut v, "print-hero", "print", &[]);
    image(
        &mut v,
        "copy-paste-1",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::T("Copy")],
    );
    image(
        &mut v,
        "copy-paste-2",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::T("Paste")],
    );
    image(
        &mut v,
        "copy-paste-hero",
        Insert,
        O::None,
        None,
        false,
        C::Ribbon,
        &[R::T("Clipboard")],
    );
    // Files.
    page(&mut v, "projects-1", Page::Info, &[R::T("Info")]);
    page(&mut v, "projects-2", Page::Info, &[R::T("Rename")]);
    page(&mut v, "projects-hero", Page::Info, &[]);
    page(&mut v, "templates-1", Page::New, &[R::T("New")]);
    page(&mut v, "templates-2", Page::New, &[R::T("Templates")]);
    page(&mut v, "templates-hero", Page::New, &[]);
    let library = || -> Setup {
        let base = image_setup(Insert, O::None, None, false);
        Box::new(move |c, e| {
            base(c, e);
            e.shell.library = true;
        })
    };
    add(
        &mut v,
        "library-1",
        false,
        library(),
        None,
        C::Tabs,
        &[R::T("Library")],
    );
    add(&mut v, "library-2", false, library(), None, C::Whole, &[]);
    add(
        &mut v,
        "library-hero",
        false,
        library(),
        None,
        C::Whole,
        &[],
    );
    // Video.
    vid(&mut v, "video-editor-hero", None, C::Whole, &[]);
    vid(&mut v, "jump-1", None, C::Canvas, &[]);
    vid(
        &mut v,
        "jump-2",
        None,
        C::Timeline,
        &[R::T("Jump to Object")],
    );
    vid(&mut v, "jump-hero", None, C::Timeline, &[]);
    vid(&mut v, "zoom-1", None, C::Ribbon, &[R::Up("Zoom")]);
    vid(&mut v, "zoom-2", None, C::Timeline, &[]);
    vid(&mut v, "zoom-3", None, C::Pane, &[]);
    vid(&mut v, "zoom-hero", None, C::Canvas, &[]);
    vid(&mut v, "keyframes-1", Some(0), C::Pane, &[R::T("Motion")]);
    vid(&mut v, "keyframes-2", Some(0), C::Timeline, &[]);
    vid(&mut v, "keyframes-3", Some(0), C::Canvas, &[]);
    vid(&mut v, "keyframes-4", Some(0), C::Pane, &[R::T("Motion")]);
    vid(&mut v, "keyframes-hero", Some(0), C::Timeline, &[]);
    vid(&mut v, "audio-1", Some(0), C::Timeline, &[]);
    vid(&mut v, "audio-2", Some(0), C::Pane, &[]);
    vid(&mut v, "audio-hero", Some(0), C::Timeline, &[]);
    image(
        &mut v,
        "gif-1",
        Export,
        O::None,
        None,
        false,
        C::Tabs,
        &[R::T("Export")],
    );
    let (_, gif_setup, gif_show) = dialog("gif-studio");
    add(
        &mut v,
        "gif-3",
        true,
        gif_setup,
        Some(gif_show),
        C::ModalTop(44.),
        &[],
    );
    let (_, gif_setup, gif_show) = dialog("gif-studio");
    add(
        &mut v,
        "gif-hero",
        true,
        gif_setup,
        Some(gif_show),
        C::ModalTop(44.),
        &[],
    );
    image(
        &mut v,
        "export-video-1",
        Export,
        O::None,
        None,
        false,
        C::Tabs,
        &[R::T("Export")],
    );
    dlg(
        &mut v,
        "export-video-2",
        "export-video",
        &[R::T("Mark all as reviewed…")],
    );
    dlg(
        &mut v,
        "export-video-3",
        "export-video",
        &[R::T("WATERMARK")],
    );
    dlg(&mut v, "export-video-hero", "export-video", &[]);
    // Scenes whose picture must show this article's own object and control.
    fn over(
        v: &mut Vec<Shot>,
        key: &str,
        video: bool,
        setup: Setup,
        show: Option<Show>,
        crop: Crop,
        rings: &[Ring],
    ) {
        v.retain(|s| s.key != key);
        add(v, key, video, setup, show, crop, rings);
    }
    let on = |what, select| image_setup(Insert, what, None, select);
    let tilted_pane = || -> Setup {
        let base = image_setup(Insert, O::ImageTilted, None, true);
        Box::new(move |c, e| {
            base(c, e);
            let a = e.session.document.annotations[0].clone();
            crate::perspective_ui::show_preset(c, a.id);
            crate::image_properties::show_image_tab(c, &a);
        })
    };
    let image_tab = |what| -> Setup {
        let base = image_setup(Insert, what, None, true);
        Box::new(move |c, e| {
            base(c, e);
            let a = e.session.document.annotations[0].clone();
            crate::image_properties::show_image_tab(c, &a);
        })
    };
    over(
        &mut v,
        "image-objects-2",
        false,
        on(O::Image, true),
        None,
        C::PaneAround(40., 170.),
        &[R::T("Appearance")],
    );
    over(
        &mut v,
        "image-objects-3",
        false,
        image_tab(O::Image),
        None,
        C::PaneAround(40., 170.),
        &[R::T("Crop image")],
    );
    over(
        &mut v,
        "perspective-2",
        false,
        image_tab(O::Image),
        None,
        C::PaneAround(40., 170.),
        &[R::T("Perspective")],
    );
    for (key, what, select, crop, rings) in [
        ("magnify-2", O::Magnify, true, C::Canvas, &[R::Object][..]),
        ("magnify-3", O::Magnify, true, C::Canvas, &[R::Object][..]),
        ("magnify-hero", O::Magnify, false, C::Canvas, &[][..]),
        ("icons-3", O::Icon, true, C::Pane, &[][..]),
        ("icons-hero", O::Icon, false, C::Canvas, &[][..]),
        (
            "import-image-3",
            O::Image,
            true,
            C::Canvas,
            &[R::Object][..],
        ),
        ("import-image-hero", O::Image, false, C::Canvas, &[][..]),
        (
            "image-objects-1",
            O::Image,
            true,
            C::Canvas,
            &[R::Object][..],
        ),
        ("image-objects-hero", O::Image, true, C::Whole, &[][..]),
        ("perspective-1", O::Image, true, C::Canvas, &[R::Object][..]),
        (
            "perspective-hero",
            O::ImageTilted,
            false,
            C::Canvas,
            &[][..],
        ),
        ("callouts-3", O::Callout, true, C::Canvas, &[R::Object][..]),
        ("move-3", O::Rect, true, C::Canvas, &[R::Object][..]),
    ] {
        over(&mut v, key, false, on(what, select), None, crop, rings);
    }
    for (key, what, ring) in [
        ("steps-3", O::Steps, "Marker"),
        ("lines-3", O::Arrow, "Appearance"),
        ("shapes-3", O::Rect, "Appearance"),
        ("colors-1", O::Rect, "Appearance"),
        ("presets-3", O::Rect, "Save current"),
    ] {
        over(
            &mut v,
            key,
            false,
            on(what, true),
            None,
            C::PaneAround(40., 170.),
            &[R::T(ring)],
        );
    }
    over(
        &mut v,
        "perspective-3",
        false,
        tilted_pane(),
        None,
        C::PaneAround(40., 130.),
        &[R::T("Angle")],
    );
    over(
        &mut v,
        "redact-3",
        false,
        image_setup(Export, O::Redact, None, false),
        None,
        C::Tabs,
        &[R::T("Export")],
    );
    let icons: Setup = image_setup(Insert, O::None, Some(ToolId::Icons), false);
    over(&mut v, "icons-2", false, icons, None, C::Pane, &[]);
    let erased: Setup = image_setup(Insert, O::Eraser, None, false);
    over(&mut v, "eraser-2", false, erased, None, C::Canvas, &[]);
    let band = image_setup(Insert, O::None, None, false);
    over(
        &mut v,
        "cutout-2",
        false,
        Box::new(move |c, e| {
            band(c, e);
            e.begin_cut_out();
            e.cut_out.as_mut().unwrap().seed_band([300, 400]);
        }),
        None,
        C::Canvas,
        &[],
    );
    let two = image_setup(Formatting, O::Two, None, true);
    over(
        &mut v,
        "arrange-1",
        false,
        Box::new(move |c, e| {
            two(c, e);
            let ids: Vec<u64> = e
                .session
                .document
                .annotations
                .iter()
                .map(|a| a.id)
                .collect();
            e.extra_selected.insert(ids[1]);
            e.selection_order.push(ids[1]);
        }),
        None,
        C::Canvas,
        &[],
    );
    // Video scenes with the object each article is about.
    let selected_object = |base: Setup| -> Setup {
        Box::new(move |c, e| {
            base(c, e);
            e.select_only(Some(2));
        })
    };
    over(
        &mut v,
        "jump-1",
        true,
        selected_object(video_setup(None)),
        None,
        C::Canvas,
        &[],
    );
    let with_zoom = || -> Setup {
        let base = video_setup(None);
        Box::new(move |c, e| {
            base(c, e);
            let p = e.session.document.video.as_mut().unwrap();
            let zoom = capture_core::video::zoom::Zoom::new(900, 2500, 7000);
            p.zooms.push(zoom);
            e.video.zoom_selected = Some(900);
        })
    };
    over(&mut v, "zoom-2", true, with_zoom(), None, C::Timeline, &[]);
    over(&mut v, "zoom-3", true, with_zoom(), None, C::Pane, &[]);
    over(&mut v, "zoom-hero", true, with_zoom(), None, C::Whole, &[]);
    let keyed = |tab: PropertyTab, on: bool| -> Setup {
        let base = video_setup(None);
        Box::new(move |c, e| {
            use capture_core::video::keyframes::{Property, Value};
            base(c, e);
            e.select_only(Some(2));
            e.video.tab = tab;
            e.video.object_tab = tab;
            e.video.keyframes.auto_key = on;
            e.video.keyframes.expand = on;
            e.video.keyframes.lanes = on;
            let t = e
                .session
                .document
                .video
                .as_mut()
                .unwrap()
                .objects
                .get_mut(&2)
                .unwrap();
            t.keyframes.insert(
                Property::Position,
                600,
                Value::Numbers(vec![220., 100.]),
                Value::Numbers(vec![320., 180.]),
            );
            t.keyframes.insert(
                Property::Position,
                1500,
                Value::Numbers(vec![420., 200.]),
                Value::Numbers(vec![320., 180.]),
            );
        })
    };
    over(
        &mut v,
        "keyframes-1",
        true,
        keyed(PropertyTab::Motion, false),
        None,
        C::Timeline,
        &[R::T("Keyframes OFF")],
    );
    over(
        &mut v,
        "keyframes-2",
        true,
        keyed(PropertyTab::Motion, true),
        None,
        C::Timeline,
        &[],
    );
    over(
        &mut v,
        "keyframes-3",
        true,
        keyed(PropertyTab::Motion, true),
        None,
        C::Pane,
        &[],
    );
    over(
        &mut v,
        "keyframes-4",
        true,
        keyed(PropertyTab::Motion, true),
        None,
        C::PaneAround(40., 130.),
        &[R::T("Transition")],
    );
    over(
        &mut v,
        "keyframes-hero",
        true,
        keyed(PropertyTab::Motion, true),
        None,
        C::Whole,
        &[],
    );
    let audio = || -> Setup {
        let base = video_setup(Some(0));
        Box::new(move |c, e| {
            base(c, e);
            let p = e.session.document.video.as_mut().unwrap();
            p.segments[0]
                .audio
                .push(capture_core::video::editor::Audio {
                    path: "recording-audio.wav".into(),
                    label: "System audio".into(),
                    volume: 1.,
                    muted: false,
                    fade_in: 0,
                    fade_out: 0,
                });
            e.video.tab = PropertyTab::Audio;
        })
    };
    over(&mut v, "audio-1", true, audio(), None, C::Timeline, &[]);
    over(
        &mut v,
        "audio-2",
        true,
        audio(),
        None,
        C::Pane,
        &[R::T("Volume")],
    );
    over(&mut v, "audio-hero", true, audio(), None, C::Pane, &[]);
    let webcam = |select: bool| -> Setup {
        let base = video_setup(None);
        Box::new(move |c, e| {
            base(c, e);
            let p = e.session.document.video.as_mut().unwrap();
            let duration = p.duration();
            capture_core::video::webcam::Webcam::default().place(
                p,
                "webcam.mp4".into(),
                [320, 320],
                duration,
            );
            let index = p.segments.len() - 1;
            let id = p.segments[index].id;
            let face = image::RgbaImage::from_fn(320, 320, |x, y| {
                let d = ((x as f32 - 160.).hypot(y as f32 - 130.) / 60.) as u8;
                if d == 0 {
                    image::Rgba([232, 190, 160, 255])
                } else if y > 210 {
                    image::Rgba([36, 92, 128, 255])
                } else {
                    image::Rgba([70, 84, 92, 255])
                }
            });
            let bitmap = Arc::new(capture_core::bitmap::Bitmap::transient(Arc::new(face)).unwrap());
            e.video.frame_layers.push((id, bitmap));
            if select {
                e.video.selected = Selection::Recording(index);
            }
        })
    };
    over(&mut v, "webcam-3", true, webcam(true), None, C::Canvas, &[]);
    over(
        &mut v,
        "webcam-hero",
        true,
        webcam(false),
        None,
        C::Canvas,
        &[],
    );
    let (_, base, show) = dialog("gif-studio");
    over(
        &mut v,
        "gif-3",
        true,
        Box::new(move |c, e| {
            base(c, e);
            let p = e.session.document.video.as_mut().unwrap();
            p.gif_ranges.push(capture_core::gif_studio::Range {
                id: 1,
                name: "Walkthrough".into(),
                start: 500,
                end: 3500,
                settings: Default::default(),
            });
            e.video.gif.selected = Some(1);
        }),
        Some(show),
        C::ModalTop(44.),
        &[],
    );
    let library_filled = || -> Setup {
        let base = library();
        Box::new(move |c, e| {
            base(c, e);
            let mut d = capture_core::Document::from_image(sample_image()).unwrap();
            d.promote_features();
            let store = e.history.store.as_mut().unwrap();
            for label in ["Account page", "Billing page", "Settings page"] {
                store
                    .create(&d, label, capture_core::history::Origin::Capture)
                    .unwrap();
            }
            e.history.refresh();
        })
    };
    over(
        &mut v,
        "library-2",
        false,
        library_filled(),
        None,
        C::Whole,
        &[],
    );
    over(
        &mut v,
        "library-hero",
        false,
        library_filled(),
        None,
        C::Whole,
        &[],
    );
    v
}
fn frame(ctx: &egui::Context, e: &mut Editor, show: Option<Show>, height: f32) -> PaintedOutput {
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH, height),
            )),
            focused: true,
            ..Default::default()
        },
        |ui| {
            crate::controls::prepare_dialog_keys(ui.ctx());
            e.show_shell(ui);
            if e.needs_render {
                e.refresh_canvas_display(ui.ctx());
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(crate::shell::CANVAS)
                        .inner_margin(12),
                )
                .show(ui, |ui| e.show_workspace(ui));
            if let Some(show) = show {
                show(ui.ctx(), e);
            }
        },
    ))
}
fn modal_rect(output: &PaintedOutput) -> Option<egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.stroke.width > 0.5 && r.stroke.color == crate::shell::ACCENT =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .max_by(|a, b| a.area().total_cmp(&b.area()))
}
fn ring_rect(e: &Editor, output: &PaintedOutput, ring: Ring) -> Option<egui::Rect> {
    let text = |label| crate::color_widgets::tests::text_rect(&output.shapes, label);
    match ring {
        Ring::T(label) => Some(text(label)?.expand2(egui::vec2(9., 5.))),
        Ring::Up(label) => {
            let r = text(label)?;
            Some(egui::Rect::from_min_max(
                egui::pos2(r.left() - 12., r.top() - 34.),
                egui::pos2(r.right() + 12., r.bottom() + 5.),
            ))
        }
        Ring::Object => {
            let t = e.view.current_transform()?;
            let (lo, hi) = e.session.document.annotations.first()?.body_bounds();
            Some(egui::Rect::from_two_pos(t.screen_point(lo), t.screen_point(hi)).expand(10.))
        }
    }
}
fn crop_rect(
    e: &Editor,
    output: &PaintedOutput,
    crop: Crop,
    first: Option<egui::Rect>,
    height: f32,
) -> [i32; 4] {
    let view = e
        .view
        .current_transform()
        .map(|t| t.viewport)
        .unwrap_or(egui::Rect::from_min_size(
            egui::pos2(0., 130.),
            egui::vec2(WIDTH - 300., height - 240.),
        ));
    let whole = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH, height));
    let r = match crop {
        Crop::Whole => whole,
        Crop::Ribbon => {
            let band = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(WIDTH, view.top()));
            first.map_or(band, |r| r.expand2(egui::vec2(240., 90.)).intersect(band))
        }
        Crop::Tabs => {
            let band = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(WIDTH, 74.));
            first.map_or(band, |r| r.expand2(egui::vec2(240., 40.)).intersect(band))
        }
        Crop::Canvas => view,
        Crop::Pane => egui::Rect::from_min_max(
            egui::pos2(view.right(), view.top()),
            egui::pos2(WIDTH, view.bottom()),
        ),
        Crop::Timeline => {
            egui::Rect::from_min_max(egui::pos2(0., view.bottom()), egui::pos2(WIDTH, height))
        }
        Crop::PaneAround(above, below) => {
            let ring = first.unwrap_or(view);
            egui::Rect::from_min_max(
                egui::pos2(view.right(), ring.top() - above),
                egui::pos2(WIDTH, ring.bottom() + below),
            )
        }
        // Some windows have no accent border, and the ribbon's is the only match.
        Crop::Modal => modal_rect(output)
            .filter(|r| r.height() > 200.)
            .unwrap_or(whole)
            .expand(4.),
        Crop::ModalTop(trim) => {
            let mut r = modal_rect(output).unwrap_or(whole).expand(4.);
            r.max.y -= trim;
            r
        }
        Crop::Around(w, h) => first.unwrap_or(whole).expand2(egui::vec2(w, h)),
    }
    .intersect(whole);
    [
        r.left() as i32,
        r.top() as i32,
        r.width().max(1.) as i32,
        r.height().max(1.) as i32,
    ]
}
/// Which step an article's picture belongs to, for its numbered ring.
fn step_numbers() -> std::collections::HashMap<String, usize> {
    let mut out = std::collections::HashMap::new();
    for a in crate::help::articles() {
        let mut n = 0;
        for b in &a.blocks {
            if let crate::help::Block::Step(_, _, pic, _) = b {
                n += 1;
                if let Some(pic) = pic {
                    out.insert(pic.clone(), n);
                }
            }
        }
    }
    out
}
#[test]
fn help_pictures_are_real_renders_of_every_article() {
    let root = std::env::var_os("CLIPCRAFT_HELP_PICTURES_DIR");
    let shots = shots();
    let keys: std::collections::HashSet<&str> = shots.iter().map(|s| s.key.as_str()).collect();
    let numbers = step_numbers();
    let unknown: Vec<&String> = numbers
        .keys()
        .filter(|k| !keys.contains(k.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "articles name pictures with no scene: {unknown:?}"
    );
    let mut missing = vec![];
    let only = std::env::var("CLIPCRAFT_HELP_ONLY").ok();
    for shot in &shots {
        if root.is_none() && shot.key.ends_with("-hero") {
            continue;
        }
        if only
            .as_ref()
            .is_some_and(|o| !o.split(',').any(|k| k == shot.key))
        {
            continue;
        }
        let ctx = egui::Context::default();
        let root_dir = tempfile::tempdir().unwrap();
        let mut e = if shot.video {
            super::repair_tests::editor(&ctx)
        } else {
            crate::shell::configure(&ctx);
            crate::text_layout::configure(&ctx);
            crate::canvas::tests::editor(&ctx)
        };
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        e.history = crate::history::HistoryPanel::isolated(root_dir.path());
        (shot.setup)(&ctx, &mut e);
        let height = if matches!(shot.crop, Crop::PaneAround(..)) {
            TALL
        } else {
            HEIGHT
        };
        let mut first = frame(&ctx, &mut e, shot.show, height);
        for _ in 0..4 {
            if e.shell.library {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            let next = frame(&ctx, &mut e, shot.show, height);
            first.0.textures_delta.append(next.textures_delta.clone());
        }
        let last = frame(&ctx, &mut e, shot.show, height);
        let rects: Vec<Option<egui::Rect>> = shot
            .rings
            .iter()
            .map(|r| ring_rect(&e, &last, *r))
            .collect();
        for (ring, r) in shot.rings.iter().zip(&rects) {
            if r.is_none() {
                let name = match ring {
                    Ring::T(t) | Ring::Up(t) => t,
                    Ring::Object => "object",
                };
                missing.push(format!("{}: {name}", shot.key));
            }
        }
        if let Some(root) = &root {
            let root = root.to_string_lossy();
            let crop = crop_rect(
                &e,
                &last,
                shot.crop,
                rects.iter().flatten().next().copied(),
                height,
            );
            let number = numbers.get(&shot.key).copied().unwrap_or(0);
            let rings: Vec<_> = rects
                .iter()
                .flatten()
                .enumerate()
                .map(|(i, r)| {
                    serde_json::json!({
                        "rect": [r.left(), r.top(), r.width(), r.height()],
                        "number": if i == 0 { number } else { 0 },
                    })
                })
                .collect();
            dump_meshes(&ctx, &root, &shot.key, first, last);
            std::fs::write(
                std::path::Path::new(&*root)
                    .join(&shot.key)
                    .join("spec.json"),
                serde_json::json!({ "crop": crop, "rings": rings }).to_string(),
            )
            .unwrap();
        } else {
            let _ = last;
            first.0.textures_delta.clear();
        }
    }
    if root.is_some() {
        assert!(
            missing.is_empty(),
            "controls not found:\n{}",
            missing.join("\n")
        );
    }
}
/// Turning the watermark on must not change the width of either dialog.
#[test]
fn export_dialogs_keep_their_width_with_the_watermark_on_or_off() {
    for name in ["export-image", "export-video"] {
        let (video, setup, show) = dialog(name);
        let ctx = egui::Context::default();
        let mut e = if video {
            super::repair_tests::editor(&ctx)
        } else {
            crate::shell::configure(&ctx);
            crate::text_layout::configure(&ctx);
            crate::canvas::tests::editor(&ctx)
        };
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        setup(&ctx, &mut e);
        let mut widths = vec![];
        for on in [false, true] {
            let mark = on.then(Default::default);
            e.export_preferences.watermark_ready = true;
            e.export_preferences.options.watermark = mark;
            if !video {
                e.begin_export(false);
            }
            let mut out = frame(&ctx, &mut e, Some(show), HEIGHT);
            for _ in 0..3 {
                out = frame(&ctx, &mut e, Some(show), HEIGHT);
            }
            widths.push(modal_rect(&out).unwrap().width());
        }
        assert!((widths[0] - widths[1]).abs() < 0.5, "{name}: {widths:?}");
    }
}
#[test]
fn every_picture_an_article_names_is_in_the_app() {
    let mut absent = vec![];
    for a in crate::help::articles() {
        for key in a
            .hero
            .iter()
            .cloned()
            .chain(a.blocks.iter().filter_map(|b| match b {
                crate::help::Block::Step(_, _, pic, _) => pic.clone(),
                _ => None,
            }))
        {
            if !crate::help::has_picture(&key) {
                absent.push(key);
            }
        }
    }
    assert!(
        absent.is_empty(),
        "pictures missing from the app: {absent:?}"
    );
}
