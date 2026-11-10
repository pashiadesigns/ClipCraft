//! Original editable guide layouts; all images retain their complete source.
mod layouts;
pub mod library;
use crate::{
    Annotation, Document, Error, Kind, Result,
    bitmap::Bitmap,
    guide_sections::{Layout, Section, SectionKind},
};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Fit,
    Fill,
}
#[derive(Clone, Copy)]
pub struct Template {
    pub id: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    /// Gallery group: Guides, Support or Announce & share (bi-proposal 2).
    pub group: &'static str,
    pub slots: usize,
    pub columns: usize,
    pub width: u32,
    pub row_height: f32,
    pub side_text: bool,
    pub timeline: bool,
    /// Placeholder text: header subtitle, per-slot headings, side body, footer.
    pub subtitle: Option<&'static str>,
    pub labels: &'static [&'static str],
    pub body: Option<&'static str>,
    pub footer: Option<&'static str>,
    pub layout: Arrangement,
    /// Numbered markers beside each slot heading.
    pub markers: bool,
}
pub const GROUPS: [&str; 3] = ["Guides", "Support", "Announce & share"];
/// How a template lays out its content (bj-proposal 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrangement {
    /// A header, rows of screenshot slots with captions, and a footer.
    Grid,
    /// One large screenshot with numbered callouts and a legend.
    Annotated,
    /// One screenshot with two Magnify insets.
    ZoomDetail,
    /// Key caps beside what they do, in two columns.
    Shortcuts,
    /// Screens left to right, joined by arrows.
    Flow,
    /// Two columns with check and cross badges.
    DoDont,
    /// The screenshot in a window frame on a color backdrop.
    Showcase,
}
const BASE: Template = Template {
    id: "",
    label: "",
    category: "",
    group: "Guides",
    slots: 1,
    columns: 1,
    width: 1200,
    row_height: 400.,
    side_text: false,
    timeline: false,
    subtitle: None,
    labels: &[],
    body: None,
    footer: None,
    layout: Arrangement::Grid,
    markers: false,
};
/// The gallery: 14 templates, each with its own job (bj-proposal 2, BJ3 review).
pub const ALL: &[Template] = &[
    Template {
        id: "steps",
        label: "Steps",
        category: "Guide",
        group: "Guides",
        slots: 4,
        columns: 1,
        width: 900,
        row_height: 260.,
        side_text: true,
        timeline: false,
        subtitle: Some("What you will do · Time needed · What you need"),
        labels: &[],
        body: Some(
            "Do this: Describe the action in one sentence.\nYou should see: Describe what changes on screen.",
        ),
        footer: None,
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "before-after",
        label: "Before / After",
        category: "Comparison",
        group: "Guides",
        slots: 2,
        columns: 2,
        width: 1200,
        row_height: 440.,
        side_text: false,
        timeline: false,
        subtitle: Some("What changed and why it matters"),
        labels: &["Before: the old screen", "After: the new screen"],
        body: None,
        footer: Some("Changed in version ______  ·  Applies to ____________________"),
        layout: Arrangement::Grid,
        markers: false,
    },
    Template {
        id: "timeline",
        label: "Timeline · Portrait",
        category: "Timeline",
        group: "Guides",
        slots: 5,
        columns: 1,
        width: 900,
        row_height: 240.,
        side_text: true,
        timeline: true,
        subtitle: None,
        labels: &[],
        body: None,
        footer: None,
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "tutorial-slide",
        label: "Tutorial slide · 16:9",
        category: "Slide",
        group: "Guides",
        slots: 1,
        columns: 1,
        width: 1280,
        row_height: 512.,
        side_text: true,
        timeline: false,
        subtitle: Some("Step 1 of 5 · Short lesson title"),
        labels: &["Click the highlighted control"],
        body: Some(
            "Explain why this step matters.\nTip: Add a shortcut or a common mistake to avoid.",
        ),
        footer: Some("Next: Name the next step"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "troubleshooting",
        label: "Troubleshooting Q&A",
        category: "Business",
        group: "Support",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 290.,
        side_text: true,
        timeline: false,
        subtitle: Some("The questions people ask, with the fix for each"),
        labels: &[
            "Symptom: What do you see?",
            "Cause: Why does it happen?",
            "Fix: What should you do?",
        ],
        body: Some(
            "What to check: Describe the evidence.\nNext action: Explain the result and where to go next.",
        ),
        footer: Some("Owner: ____________________  ·  Version: ______  ·  Reviewed: __________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "checklist",
        label: "Checklist",
        category: "Support",
        group: "Support",
        slots: 4,
        columns: 2,
        width: 1200,
        row_height: 300.,
        side_text: false,
        timeline: false,
        subtitle: Some("Tick each item when it is done"),
        labels: &[
            "☐ First item",
            "☐ Second item",
            "☐ Third item",
            "☐ Fourth item",
        ],
        body: None,
        footer: Some("Completed by: ____________________  ·  Date: __________"),
        layout: Arrangement::Grid,
        markers: false,
    },
    Template {
        id: "release-notes",
        label: "Release notes",
        category: "Announcement",
        group: "Announce & share",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 290.,
        side_text: true,
        timeline: false,
        subtitle: Some("Version ______ · Released __________"),
        labels: &[
            "New: Name the feature",
            "Improved: Name the change",
            "Fixed: Name the problem",
        ],
        body: Some(
            "Describe the change in one or two sentences.\nSay who benefits and how to try it.",
        ),
        footer: Some("Full change list: ____________________"),
        layout: Arrangement::Grid,
        markers: false,
    },
    Template {
        id: "social-card",
        label: "Social card · Square",
        category: "Announcement",
        group: "Announce & share",
        slots: 1,
        columns: 1,
        width: 1080,
        row_height: 872.,
        side_text: false,
        timeline: false,
        subtitle: Some("Add a short supporting line"),
        labels: &["Write a headline"],
        body: None,
        footer: Some("yourwebsite.example"),
        layout: Arrangement::Grid,
        markers: false,
    },
    Template {
        id: "annotated-screen",
        label: "Annotated screen",
        category: "Annotated",
        group: "Guides",
        slots: 1,
        width: 1200,
        row_height: 600.,
        subtitle: Some("Point out the parts of one screen"),
        layout: Arrangement::Annotated,
        ..BASE
    },
    Template {
        id: "zoom-detail",
        label: "Zoom detail",
        category: "Annotated",
        group: "Guides",
        slots: 1,
        width: 1200,
        row_height: 600.,
        subtitle: Some("The full screen, with the small parts that matter enlarged"),
        layout: Arrangement::ZoomDetail,
        ..BASE
    },
    Template {
        id: "shortcut-sheet",
        label: "Shortcut sheet",
        category: "Reference",
        group: "Support",
        slots: 0,
        width: 1200,
        row_height: 520.,
        subtitle: Some("Keyboard shortcuts at a glance"),
        layout: Arrangement::Shortcuts,
        ..BASE
    },
    Template {
        id: "process-flow",
        label: "Process flow",
        category: "Steps",
        group: "Guides",
        slots: 3,
        width: 1200,
        row_height: 420.,
        subtitle: Some("How the work moves from one screen to the next"),
        layout: Arrangement::Flow,
        ..BASE
    },
    Template {
        id: "do-dont",
        label: "Do / Don't",
        category: "Comparison",
        group: "Support",
        slots: 2,
        width: 1200,
        row_height: 540.,
        subtitle: Some("The right way, and the mistake to avoid"),
        layout: Arrangement::DoDont,
        ..BASE
    },
    Template {
        id: "window-showcase",
        label: "Window showcase",
        category: "Announcement",
        group: "Announce & share",
        slots: 1,
        width: 1200,
        row_height: 600.,
        subtitle: Some("Add a short supporting line"),
        footer: Some("yourwebsite.example"),
        layout: Arrangement::Showcase,
        ..BASE
    },
];
/// Templates folded into others in BJ. They stay buildable by id (projects
/// made from them are ordinary documents) but are no longer in the gallery.
const RETIRED: &[Template] = &[
    Template {
        id: "issue-report",
        label: "Issue report",
        category: "Support",
        group: "Support",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 280.,
        side_text: true,
        timeline: false,
        subtitle: Some("Status: Bug · Known issue · Fixed  ·  Product · Version · Reported by"),
        labels: &[
            "Steps to reproduce",
            "Expected and actual result",
            "Workaround",
        ],
        body: Some(
            "Describe exactly what you did or saw.\nInclude error text, times and file names.",
        ),
        footer: Some("Severity: ______  ·  Frequency: ______  ·  Ticket: __________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "quick-reference",
        label: "Quick reference",
        category: "Business",
        group: "Support",
        slots: 4,
        columns: 2,
        width: 1200,
        row_height: 320.,
        side_text: false,
        timeline: false,
        subtitle: Some("Essential actions at a glance"),
        labels: &[
            "Navigation",
            "Common actions",
            "Keyboard shortcuts",
            "Get help",
        ],
        body: None,
        footer: Some("Owner: ____________________  ·  Version: ______  ·  Reviewed: __________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "feature-spotlight",
        label: "Feature spotlight",
        category: "Announcement",
        group: "Announce & share",
        slots: 1,
        columns: 1,
        width: 1200,
        row_height: 560.,
        side_text: true,
        timeline: false,
        subtitle: Some("One feature, explained in a minute"),
        labels: &["Name the feature"],
        body: Some(
            "What it does: One sentence.\nTip 1: Where to find it.\nTip 2: A shortcut.\nTip 3: A common use.",
        ),
        footer: Some("Learn more: ____________________"),
        layout: Arrangement::Grid,
        markers: false,
    },
    Template {
        id: "two-steps",
        label: "Two steps · Portrait",
        category: "Steps",
        group: "Guides",
        slots: 2,
        columns: 1,
        width: 900,
        row_height: 370.,
        side_text: false,
        timeline: false,
        subtitle: None,
        labels: &[],
        body: None,
        footer: None,
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "three-steps",
        label: "Three steps · Portrait",
        category: "Steps",
        group: "Guides",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 270.,
        side_text: true,
        timeline: false,
        subtitle: None,
        labels: &[],
        body: None,
        footer: None,
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "four-steps",
        label: "Four steps · Grid",
        category: "Steps",
        group: "Guides",
        slots: 4,
        columns: 2,
        width: 900,
        row_height: 320.,
        side_text: false,
        timeline: false,
        subtitle: None,
        labels: &[],
        body: None,
        footer: None,
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "comparison",
        label: "Before / After · Landscape",
        category: "Comparison",
        group: "Guides",
        slots: 2,
        columns: 2,
        width: 1200,
        row_height: 420.,
        side_text: false,
        timeline: false,
        subtitle: None,
        labels: &["Before", "After"],
        body: None,
        footer: None,
        layout: Arrangement::Grid,
        markers: false,
    },
    Template {
        id: "overview",
        label: "Overview · Landscape",
        category: "Overview",
        group: "Announce & share",
        slots: 1,
        columns: 1,
        width: 1200,
        row_height: 440.,
        side_text: true,
        timeline: false,
        subtitle: None,
        labels: &["Key information"],
        body: None,
        footer: None,
        layout: Arrangement::Grid,
        markers: false,
    },
    Template {
        id: "sop",
        label: "Standard operating procedure",
        category: "Business",
        group: "Guides",
        slots: 4,
        columns: 1,
        width: 900,
        row_height: 275.,
        side_text: true,
        timeline: false,
        subtitle: Some("Purpose · Scope · Owner · Last reviewed"),
        labels: &[
            "Purpose & prerequisites",
            "Prepare",
            "Perform the procedure",
            "Verify the result",
        ],
        body: Some(
            "Action: Describe what to do.\nExpected result: Explain how to confirm success.",
        ),
        footer: Some("Owner: ____________________  ·  Version: ______  ·  Reviewed: __________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "onboarding",
        label: "Onboarding checklist",
        category: "Business",
        group: "Guides",
        slots: 4,
        columns: 1,
        width: 900,
        row_height: 250.,
        side_text: true,
        timeline: false,
        subtitle: Some("Name: ____________________   Team: ____________________"),
        labels: &[
            "Account & access",
            "Set up your workspace",
            "Practice the workflow",
            "Review & sign off",
        ],
        body: Some("Task: Describe the activity.\nCompleted: __________  Reviewed by: __________"),
        footer: Some("Owner: ____________________  ·  Version: ______  ·  Reviewed: __________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "training",
        label: "Training handout",
        category: "Business",
        group: "Guides",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 290.,
        side_text: true,
        timeline: false,
        subtitle: Some("Learning objective · Audience · Prerequisites"),
        labels: &["Learn", "Practice", "Check your understanding"],
        body: Some(
            "Explain the concept and show an example.\nPractice: Add a task for the learner.",
        ),
        footer: Some("Owner: ____________________  ·  Version: ______  ·  Reviewed: __________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "how-to",
        label: "How-to",
        category: "Guide",
        group: "Guides",
        slots: 4,
        columns: 1,
        width: 900,
        row_height: 260.,
        side_text: true,
        timeline: false,
        subtitle: Some("What you will do · Time needed · What you need"),
        labels: &[
            "Open the starting point",
            "Make the change",
            "Save or confirm",
            "Check the result",
        ],
        body: Some(
            "Do this: Describe the action in one sentence.\nYou should see: Describe what changes on screen.",
        ),
        footer: None,
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "bug-report",
        label: "Bug report",
        category: "Support",
        group: "Support",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 280.,
        side_text: true,
        timeline: false,
        subtitle: Some("Product · Version · Operating system · Reported by"),
        labels: &["Steps to reproduce", "Expected result", "Actual result"],
        body: Some(
            "Describe exactly what you did or saw.\nInclude error text, times and file names.",
        ),
        footer: Some("Severity: ______  ·  Frequency: ______  ·  Ticket: __________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "known-issue",
        label: "Known issue",
        category: "Support",
        group: "Support",
        slots: 2,
        columns: 1,
        width: 900,
        row_height: 320.,
        side_text: true,
        timeline: false,
        subtitle: Some("Status: Investigating · Last updated: __________"),
        labels: &["Who is affected", "Workaround"],
        body: Some(
            "Describe the symptom and when it happens.\nList the steps that avoid the problem for now.",
        ),
        footer: Some("Fix expected in: __________  ·  Contact: ____________________"),
        layout: Arrangement::Grid,
        markers: true,
    },
    Template {
        id: "faq",
        label: "FAQ",
        category: "Support",
        group: "Support",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 270.,
        side_text: true,
        timeline: false,
        subtitle: Some("Answers to common questions"),
        labels: &[
            "Question: How do I…?",
            "Question: Where can I find…?",
            "Question: What happens when…?",
        ],
        body: Some("Answer: Give the short answer first.\nThen show where to click."),
        footer: None,
        layout: Arrangement::Grid,
        markers: false,
    },
];
pub fn find(id: &str) -> Option<&'static Template> {
    ALL.iter().chain(RETIRED).find(|t| t.id == id)
}
/// The Steps template with its step count (2–8) and Stacked or Grid layout,
/// chosen in the gallery before it opens.
pub fn steps(count: usize, grid: bool) -> Result<Template> {
    if !(2..=8).contains(&count) {
        return Err(Error::Invalid("Choose 2 to 8 steps.".into()));
    }
    let base = *find("steps").expect("built-in Steps template");
    Ok(Template {
        slots: count,
        columns: if grid { 2 } else { 1 },
        row_height: if grid { 320. } else { 260. },
        side_text: !grid,
        ..base
    })
}
pub fn placeholder() -> Result<Arc<Bitmap>> {
    let image = image::RgbaImage::from_fn(240, 150, |x, y| {
        let mountain = (40..205).contains(&x)
            && y > 115_u32.saturating_sub((x as i32 - 110).unsigned_abs() / 2)
            && y < 125;
        let sun = (x as i32 - 179).pow(2) + (y as i32 - 43).pow(2) < 12 * 12;
        image::Rgba(if mountain || sun {
            [160, 174, 180, 255]
        } else {
            [232, 238, 241, 255]
        })
    });
    Ok(Arc::new(
        Bitmap::new(Arc::new(crate::RasterImage::from_image(&image)?))?.into_placeholder(),
    ))
}
/// Reusable for creation and replacing an existing frame; crop stays nondestructive.
pub fn image_in_frame(
    bitmap: Arc<Bitmap>,
    id: u64,
    rect: [f32; 4],
    scale: Scale,
) -> Result<Annotation> {
    let [x, y, w, h] = rect;
    if !rect.into_iter().all(f32::is_finite) || w <= 0. || h <= 0. {
        return Err(Error::Invalid("Invalid image frame".into()));
    }
    let [iw, ih] = bitmap.size().map(|v| v as f32);
    let mut a = bitmap.clone().annotation(id);
    match scale {
        Scale::Fit => {
            let factor = (w / iw).min(h / ih);
            let [dw, dh] = [iw * factor, ih * factor];
            a.start = [x + (w - dw) * 0.5, y + (h - dh) * 0.5];
            a.end = [a.start[0] + dw, a.start[1] + dh];
        }
        Scale::Fill => {
            let ratio = (w / h) / (iw / ih);
            let [cw, ch] = if ratio < 1. {
                [ratio, 1.]
            } else {
                [1., 1. / ratio]
            };
            a.image = Some(Arc::new(bitmap.with_view(Some(
                crate::image_frame::ImageView {
                    rect: [(1. - cw) * 0.5, (1. - ch) * 0.5, cw, ch],
                },
            ))?));
            a.start = [x, y];
            a.end = [x + w, y + h];
        }
    }
    a.corner_radius = 8.;
    Ok(a)
}
pub(crate) fn text(id: u64, rect: [f32; 4], content: &str, size: f32, bold: bool) -> Annotation {
    Annotation {
        id,
        kind: Kind::Text,
        start: [rect[0], rect[1]],
        end: [rect[0] + rect[2], rect[1] + rect[3]],
        color: [40, 47, 52, 255],
        width: 1,
        corner_radius: 0.,
        fill: None,
        points: vec![],
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,

        focus: None,

        highlighter_rect: false,
        hard_edge: false,
        group: None,
        rotation_degrees: 0.,
        mirror_x: false,
        callout: None,
        text: Some(crate::text::TextStyle {
            content: content.into(),
            size_points: size,
            bold,
            horizontal: crate::text::HorizontalAlignment::Left,
            vertical: crate::text::VerticalAlignment::Top,
            ..Default::default()
        }),
    }
}
pub fn build(id: &str, title: &str, images: &[Arc<Bitmap>], scale: Scale) -> Result<Document> {
    let t = find(id).ok_or_else(|| Error::Invalid("Unknown image template".into()))?;
    build_template(t, title, images, scale)
}
pub fn build_template(
    t: &Template,
    title: &str,
    images: &[Arc<Bitmap>],
    scale: Scale,
) -> Result<Document> {
    if t.layout != Arrangement::Grid {
        return layouts::build(t, title, images, scale);
    }
    if images.len() > t.slots || title.chars().count() > 120 {
        return Err(Error::Invalid(format!(
            "This template accepts at most {} images and a title of 120 characters.",
            t.slots
        )));
    }
    let rows = t.slots.div_ceil(t.columns);
    let height = 144. + rows as f32 * t.row_height + 64.;
    let mut d = Document::canvas(t.width, height.ceil() as u32, [255; 4])?;
    let mut sections = vec![];
    let mut next = 1;
    let mut add = |d: &mut Document, mut a: Annotation| {
        a.id = next;
        next += 1;
        let id = a.id;
        d.annotations.push(a);
        id
    };
    let header = vec![
        add(
            &mut d,
            text(
                0,
                [32., 25., t.width as f32 - 64., 58.],
                if title.trim().is_empty() {
                    "Untitled guide"
                } else {
                    title
                },
                32.,
                true,
            ),
        ),
        add(
            &mut d,
            text(
                0,
                [32., 92., t.width as f32 - 64., 35.],
                t.subtitle.unwrap_or("Add a subtitle or introduction"),
                17.,
                false,
            ),
        ),
    ];
    sections.push(Section {
        page: 0,
        id: 1,
        kind: SectionKind::Header,
        height: 144.,
        objects: header,
        marker: None,
    });
    let placeholder = placeholder()?;
    for row in 0..rows {
        let top = 144. + row as f32 * t.row_height;
        let mut objects = vec![];
        let mut marker = None;
        for col in 0..t.columns {
            let index = row * t.columns + col;
            if index >= t.slots {
                break;
            }
            let cw = (t.width as f32 - 64. - (t.columns - 1) as f32 * 24.) / t.columns as f32;
            let left = 32. + col as f32 * (cw + 24.);
            let side = t.side_text;
            let image_w = if side { cw * 0.58 } else { cw };
            let image_h = if side {
                t.row_height - 40.
            } else {
                t.row_height - 90.
            };
            let image_left = if t.timeline && row % 2 == 1 {
                t.width as f32 - 32. - image_w
            } else {
                left
            };
            let bitmap = images.get(index).unwrap_or(&placeholder).clone();
            objects.push(add(
                &mut d,
                image_in_frame(bitmap, 0, [image_left, top + 16., image_w, image_h], scale)?,
            ));
            let caption_x = if side {
                if t.timeline && row % 2 == 1 {
                    left
                } else {
                    left + image_w + 24.
                }
            } else {
                left
            };
            let caption_y = if side { top + 30. } else { top + image_h + 30. };
            let caption_w = if side { cw - image_w - 24. } else { cw };
            let label = t
                .labels
                .get(index)
                .map_or_else(|| format!("Step {}", index + 1), |l| (*l).to_owned());
            let marker_id = if t.markers {
                let a = crate::steps::marker(
                    1,
                    [caption_x + 14., caption_y + 14.],
                    28,
                    [24, 145, 193, 255],
                    ((index + 1) as u32, false),
                    [d.width, d.height],
                )?;
                Some(add(&mut d, a))
            } else {
                None
            };
            if let Some(id) = marker_id {
                objects.push(id);
                if t.columns == 1 {
                    marker = Some(id);
                }
            }
            let offset = if marker_id.is_some() { 38. } else { 0. };
            objects.push(add(
                &mut d,
                text(
                    0,
                    [caption_x + offset, caption_y, caption_w - offset, 32.],
                    &label,
                    20.,
                    true,
                ),
            ));
            if side {
                objects.push(add(
                    &mut d,
                    text(
                        0,
                        [caption_x, caption_y + 46., caption_w, t.row_height - 110.],
                        t.body.unwrap_or("Describe the action or key point."),
                        17.,
                        false,
                    ),
                ));
            }
        }
        sections.push(Section {
            page: 0,
            id: row as u64 + 2,
            kind: SectionKind::Image,
            height: t.row_height,
            objects,
            marker,
        });
    }
    let footer = add(
        &mut d,
        text(
            0,
            [32., height - 44., t.width as f32 - 64., 28.],
            t.footer.unwrap_or("Created with ClipDesigner"),
            13.,
            false,
        ),
    );
    sections.push(Section {
        page: 0,
        id: rows as u64 + 2,
        kind: SectionKind::Text,
        height: 64.,
        objects: vec![footer],
        marker: None,
    });
    d.guide = Some(Layout {
        pages: None,
        template: Some(t.id.into()),
        top: 0.,
        sections,
    });
    d.promote_features();
    d.validate()?;
    Ok(d)
}
#[cfg(test)]
mod tests {
    use super::*;
    /// bj-proposal 2 and the BJ3 review: 8 kept + 6 new layouts; Steps takes 2–8 steps,
    /// stacked or as a grid; retired templates still build by id.
    #[test]
    fn gallery_is_consolidated_and_steps_are_configurable() {
        assert_eq!(ALL.len(), 14);
        let ids: Vec<_> = ALL.iter().map(|t| t.id).collect();
        for gone in ["two-steps", "how-to", "faq", "comparison", "bug-report"] {
            assert!(!ids.contains(&gone), "{gone} left the gallery");
            assert!(find(gone).is_some(), "{gone} still builds");
        }
        for (count, grid) in [(2, false), (5, false), (8, true)] {
            let t = steps(count, grid).unwrap();
            let d = build_template(&t, "Guide", &[], Scale::Fit).unwrap();
            let frames = d
                .annotations
                .iter()
                .filter(|a| a.kind == Kind::Image)
                .count();
            assert_eq!(frames, count);
            let lefts: std::collections::BTreeSet<i32> = d
                .annotations
                .iter()
                .filter(|a| a.kind == Kind::Image)
                .map(|a| a.start[0] as i32)
                .collect();
            assert_eq!(lefts.len(), if grid { 2 } else { 1 }, "{count} {grid}");
        }
        assert!(steps(1, false).is_err() && steps(9, true).is_err());
        // Each new layout is its own look: no two render alike.
        let looks: Vec<_> = ALL
            .iter()
            .filter(|t| t.layout != Arrangement::Grid)
            .map(|t| {
                build(t.id, "Guide", &[], Scale::Fit)
                    .unwrap()
                    .render()
                    .unwrap()
            })
            .collect();
        assert_eq!(looks.len(), 6);
        for (i, a) in looks.iter().enumerate() {
            for b in &looks[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
    #[test]
    fn templates_keep_editable_sections_sources_and_exact_roundtrips() {
        for t in ALL {
            for scale in [Scale::Fit, Scale::Fill] {
                let source = Arc::new(
                    Bitmap::new(Arc::new(
                        crate::RasterImage::from_image(&image::RgbaImage::from_pixel(
                            640,
                            180,
                            image::Rgba([38, 110, 180, 255]),
                        ))
                        .unwrap(),
                    ))
                    .unwrap(),
                );
                let given: &[Arc<Bitmap>] = if t.slots == 0 {
                    &[]
                } else {
                    std::slice::from_ref(&source)
                };
                let d = build(t.id, "Deployment guide", given, scale).unwrap();
                assert_eq!(d.version, crate::MAX_PROJECT_VERSION);
                // Picture frames, not icon badges.
                assert_eq!(
                    d.annotations
                        .iter()
                        .filter(|a| a.kind == Kind::Image && !a.image.as_ref().unwrap().is_icon())
                        .count(),
                    t.slots,
                    "{}",
                    t.id
                );
                let saved = serde_json::to_vec(&d).unwrap();
                let copy: Document = serde_json::from_slice(&saved).unwrap();
                copy.validate().unwrap();
                assert_eq!(d.render().unwrap(), copy.render().unwrap());
                if let Some(first) = d
                    .annotations
                    .iter()
                    .find(|a| a.kind == Kind::Image && !a.image.as_ref().unwrap().is_icon())
                {
                    assert_eq!(first.image.as_ref().unwrap().size(), source.size());
                }
                let mut s = crate::Session::new(d.clone()).unwrap();
                s.execute(crate::Command::Guide {
                    action: crate::guide_sections::Action::Duplicate { id: 2 },
                })
                .unwrap();
                s.execute(crate::Command::Undo).unwrap();
                assert_eq!(serde_json::to_vec(&s.document).unwrap(), saved);
                let root = tempfile::tempdir().unwrap();
                let mut h = crate::history::History::open(root.path()).unwrap();
                let e = h
                    .create(&d, "Template", crate::history::Origin::Canvas)
                    .unwrap();
                assert_eq!(e.kind, crate::history::MediaKind::Template);
                let mut reopened = crate::history::History::open(root.path()).unwrap();
                assert_eq!(
                    reopened.entries()[0].kind,
                    crate::history::MediaKind::Template
                );
                let duplicated = reopened.duplicate(&e).unwrap();
                assert_eq!(duplicated.kind, crate::history::MediaKind::Template);
                let mut legacy = d.clone();
                legacy.version = 23;
                assert!(legacy.validate().is_err());
                if scale == Scale::Fill
                    && let Ok(root) = std::env::var("CLIPDESIGNER_TEMPLATE_FIXTURES")
                {
                    std::fs::create_dir_all(&root).unwrap();
                    d.save(
                        &std::path::Path::new(&root)
                            .join(format!("template-{}.capture.json", t.id)),
                    )
                    .unwrap();
                }
            }
        }
        assert!(build("unknown", "", &[], Scale::Fit).is_err());
        // Every gallery template is in a gallery group (14 after the BJ3 review).
        assert_eq!(ALL.len(), 14);
        assert!(ALL.iter().all(|t| GROUPS.contains(&t.group)));
        let slide = build("tutorial-slide", "", &[], Scale::Fit).unwrap();
        assert_eq!((slide.width, slide.height), (1280, 720));
        let card = build("social-card", "", &[], Scale::Fit).unwrap();
        assert_eq!((card.width, card.height), (1080, 1080));
        assert!(build("overview", "", &vec![placeholder().unwrap(); 2], Scale::Fit).is_err());
    }
}
