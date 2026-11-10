//! The six non-grid template layouts (approved bj-proposal 2): Annotated
//! screen, Zoom detail, Shortcut sheet, Process flow, Do / Don't and Window
//! showcase. Each is a header, one body section and a footer, built from
//! ordinary editable objects.
use super::{Arrangement, Scale, Template, image_in_frame, placeholder, text};
use crate::{
    Annotation, Document, Error, Kind, Result,
    bitmap::Bitmap,
    guide_sections::{Layout, Section, SectionKind},
};
use std::sync::Arc;

const ACCENT: [u8; 4] = [24, 145, 193, 255];
const INK: [u8; 4] = [40, 47, 52, 255];
const HEADER: f32 = 144.;
const FOOTER: f32 = 64.;

struct Builder {
    doc: Document,
    next: u64,
    body: Vec<u64>,
}
impl Builder {
    fn add(&mut self, mut a: Annotation) -> u64 {
        a.id = self.next;
        self.next += 1;
        let id = a.id;
        self.doc.annotations.push(a);
        id
    }
    fn body(&mut self, a: Annotation) {
        let id = self.add(a);
        self.body.push(id);
    }
}
/// A plain shape derived from the text helper (same defaults, no text).
fn shape(kind: Kind, rect: [f32; 4], color: [u8; 4], width: u32) -> Annotation {
    let mut a = text(0, rect, "", 1., false);
    a.kind = kind;
    a.text = None;
    a.color = color;
    a.width = width;
    a
}
fn line(kind: Kind, from: [f32; 2], to: [f32; 2], width: u32) -> Annotation {
    let mut a = shape(kind, [0., 0., 1., 1.], ACCENT, width);
    a.start = from;
    a.end = to;
    a
}
fn caption(rect: [f32; 4], title: &str, body: &str) -> [Annotation; 2] {
    [
        text(0, [rect[0], rect[1], rect[2], 30.], title, 19., true),
        text(
            0,
            [rect[0], rect[1] + 34., rect[2], (rect[3] - 34.).max(24.)],
            body,
            15.,
            false,
        ),
    ]
}
pub(super) fn build(
    t: &Template,
    title: &str,
    images: &[Arc<Bitmap>],
    scale: Scale,
) -> Result<Document> {
    if images.len() > t.slots || title.chars().count() > 120 {
        return Err(Error::Invalid(format!(
            "This template accepts at most {} images and a title of 120 characters.",
            t.slots
        )));
    }
    let w = t.width as f32;
    let body_h = t.row_height;
    let height = HEADER + body_h + FOOTER;
    let showcase = t.layout == Arrangement::Showcase;
    let mut b = Builder {
        doc: Document::canvas(
            t.width,
            height.ceil() as u32,
            if showcase { ACCENT } else { [255; 4] },
        )?,
        next: 1,
        body: vec![],
    };
    let light = |mut a: Annotation| {
        if showcase {
            a.color = [255; 4];
        }
        a
    };
    let header = vec![
        b.add(light(text(
            0,
            [32., 25., w - 64., 58.],
            if title.trim().is_empty() {
                "Untitled guide"
            } else {
                title
            },
            32.,
            true,
        ))),
        b.add(light(text(
            0,
            [32., 92., w - 64., 35.],
            t.subtitle.unwrap_or("Add a subtitle or introduction"),
            17.,
            false,
        ))),
    ];
    let placeholder = placeholder()?;
    let image = |i: usize| images.get(i).unwrap_or(&placeholder).clone();
    let top = HEADER;
    let canvas = [b.doc.width, b.doc.height];
    match t.layout {
        Arrangement::Annotated => {
            // Each numbered label carries its own text, once, and leads into
            // the picture (BJ3 review).
            let frame = [300., top + 20., 600., body_h - 40.];
            b.body(image_in_frame(image(0), 0, frame, scale)?);
            let rows = [frame[1] + 90., frame[1] + 330.];
            let items = [
                (rows[0], true, [frame[0] + 140., rows[0] + 30.]),
                (rows[0], false, [frame[0] + 470., rows[0] + 60.]),
                (rows[1], true, [frame[0] + 180., rows[1]]),
                (rows[1], false, [frame[0] + 430., rows[1] + 30.]),
            ];
            for (i, (y, left, into)) in items.into_iter().enumerate() {
                let at = if left {
                    [frame[0] - 20., y + 16.]
                } else {
                    [frame[0] + frame[2] + 20., y + 16.]
                };
                b.body(line(Kind::Line, at, into, 3));
                b.body(crate::steps::marker(
                    1,
                    at,
                    32,
                    ACCENT,
                    (i as u32 + 1, false),
                    canvas,
                )?);
                let x = if left { 32. } else { frame[0] + frame[2] + 44. };
                let width = frame[0] - 44. - 32.;
                for a in caption(
                    [x, y, width, 110.],
                    "Name this part",
                    "Say what it does or when to use it.",
                ) {
                    b.body(a);
                }
            }
        }
        Arrangement::ZoomDetail => {
            let frame = [32., top + 20., 720., body_h - 40.];
            b.body(image_in_frame(image(0), 0, frame, scale)?);
            // Two Magnify callouts whose enlarged views sit right of the screen.
            for (i, (source, lens_y)) in [
                ([frame[0] + 140., frame[1] + 90.], top + 130.),
                ([frame[0] + 380., frame[1] + 360.], top + 400.),
            ]
            .into_iter()
            .enumerate()
            {
                let size = [120., 80.];
                let center = [source[0] + size[0] * 0.5, source[1] + size[1] * 0.5];
                let mut a = shape(
                    Kind::Rectangle,
                    [source[0], source[1], size[0], size[1]],
                    ACCENT,
                    3,
                );
                a.corner_radius = 6.;
                a.focus = Some(crate::focus::Focus {
                    magnify: Some(crate::magnified::Magnified {
                        offset: [(990. - center[0]) / size[0], (lens_y - center[1]) / size[1]],
                        scale: 2.5,
                        connectors: true,
                    }),
                    blur: 0.,
                    darken: 0.,
                    opacity: 1.,
                });
                b.body(a);
                b.body(text(
                    0,
                    [840., lens_y + 108., w - 872., 30.],
                    if i == 0 {
                        "Detail 1: say what to notice"
                    } else {
                        "Detail 2: say what to notice"
                    },
                    16.,
                    true,
                ));
            }
        }
        Arrangement::Shortcuts => {
            let keys: [(&[&str], &str); 12] = [
                (&["Ctrl", "C"], "Copy"),
                (&["Ctrl", "V"], "Paste"),
                (&["Ctrl", "Z"], "Undo"),
                (&["Ctrl", "Y"], "Redo"),
                (&["Ctrl", "S"], "Save"),
                (&["Ctrl", "F"], "Find"),
                (&["F1"], "Help"),
                (&["F2"], "Rename"),
                (&["Esc"], "Cancel"),
                (&["Ctrl", "A"], "Select all"),
                (&["Del"], "Delete"),
                (&["Ctrl", "P"], "Print"),
            ];
            for (i, (caps, action)) in keys.iter().enumerate() {
                let col = (i / 6) as f32;
                let row = (i % 6) as f32;
                let x = 48. + col * (w - 96.) * 0.5;
                let y = top + 24. + row * 80.;
                let mut cx = x;
                for cap in caps.iter() {
                    let cw = 34. + cap.chars().count() as f32 * 11.;
                    let mut key = text(0, [cx, y, cw, 46.], cap, 17., true);
                    key.kind = Kind::Rectangle;
                    key.fill = Some([255; 4]);
                    key.color = [168, 183, 188, 255];
                    key.width = 2;
                    key.corner_radius = 7.;
                    if let Some(s) = key.text.as_mut() {
                        s.horizontal = crate::text::HorizontalAlignment::Center;
                        s.vertical = crate::text::VerticalAlignment::Middle;
                        s.color = Some(INK);
                    }
                    b.body(key);
                    cx += cw + 10.;
                }
                b.body(text(0, [x + 190., y + 8., 300., 32.], action, 19., false));
            }
        }
        Arrangement::Flow => {
            let n = t.slots;
            let gap = 72.;
            let cw = (w - 64. - gap * (n - 1) as f32) / n as f32;
            for i in 0..n {
                let x = 32. + i as f32 * (cw + gap);
                let frame = [x, top + 56., cw, cw * 0.66];
                b.body(crate::steps::marker(
                    1,
                    [x + 16., top + 28.],
                    30,
                    ACCENT,
                    (i as u32 + 1, false),
                    canvas,
                )?);
                b.body(image_in_frame(image(i), 0, frame, scale)?);
                if i + 1 < n {
                    let y = frame[1] + frame[3] * 0.5;
                    b.body(line(
                        Kind::Arrow,
                        [x + cw + 12., y],
                        [x + cw + gap - 12., y],
                        5,
                    ));
                }
                for a in caption(
                    [x, frame[1] + frame[3] + 18., cw, 90.],
                    &format!("Step {}", i + 1),
                    "Describe what happens on this screen.",
                ) {
                    b.body(a);
                }
            }
        }
        Arrangement::DoDont => {
            let cw = (w - 64. - 32.) * 0.5;
            for (i, (title, body, icon)) in [
                (
                    "Do: the right way",
                    "Show the correct result and why it works.",
                    "builtin-check-circle",
                ),
                (
                    "Don't: the mistake",
                    "Show what goes wrong and how to avoid it.",
                    "builtin-cross-circle",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let x = 32. + i as f32 * (cw + 32.);
                let frame = [x, top + 20., cw, 340.];
                b.body(image_in_frame(image(i), 0, frame, scale)?);
                let mut badge = crate::icon_library::builtin::find(icon)
                    .ok_or_else(|| Error::Invalid("Missing built-in icon".into()))?
                    .payload()?
                    .objects
                    .remove(0);
                badge.start = [x + 12., top + 32.];
                badge.end = [x + 60., top + 80.];
                b.body(badge);
                for a in caption([x, top + 380., cw, 120.], title, body) {
                    b.body(a);
                }
            }
        }
        Arrangement::Showcase => {
            let win = [120., top + 16., w - 240., body_h - 40.];
            let mut window = shape(Kind::Rectangle, win, [255; 4], 1);
            window.fill = Some([255; 4]);
            window.corner_radius = 12.;
            window.shadow = Some(Default::default());
            b.body(window);
            let mut bar = shape(
                Kind::Rectangle,
                [win[0], win[1], win[2], 30.],
                [223, 230, 233, 255],
                1,
            );
            bar.fill = Some([223, 230, 233, 255]);
            bar.corner_radius = 12.;
            b.body(bar);
            for (i, c) in [[237, 106, 94, 255], [245, 191, 79, 255], [98, 197, 84, 255]]
                .into_iter()
                .enumerate()
            {
                let mut dot = shape(
                    Kind::Ellipse,
                    [win[0] + 14. + i as f32 * 18., win[1] + 9., 12., 12.],
                    c,
                    1,
                );
                dot.fill = Some(c);
                b.body(dot);
            }
            b.body(image_in_frame(
                image(0),
                0,
                [win[0] + 8., win[1] + 38., win[2] - 16., win[3] - 46.],
                scale,
            )?);
        }
        Arrangement::Grid => unreachable!("grid templates use the slot builder"),
    }
    let body = std::mem::take(&mut b.body);
    let footer = b.add(light(text(
        0,
        [32., height - 44., w - 64., 28.],
        t.footer.unwrap_or("Created with ClipDesigner"),
        13.,
        false,
    )));
    let mut d = b.doc;
    d.guide = Some(Layout {
        pages: None,
        template: Some(t.id.into()),
        top: 0.,
        sections: vec![
            Section {
                page: 0,
                id: 1,
                kind: SectionKind::Header,
                height: HEADER,
                objects: header,
                marker: None,
            },
            Section {
                page: 0,
                id: 2,
                kind: SectionKind::Image,
                height: body_h,
                objects: body,
                marker: None,
            },
            Section {
                page: 0,
                id: 3,
                kind: SectionKind::Text,
                height: FOOTER,
                objects: vec![footer],
                marker: None,
            },
        ],
    });
    d.promote_features();
    d.validate()?;
    Ok(d)
}
