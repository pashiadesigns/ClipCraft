//! Help pictures drawn by ClipDesigner's own renderer, so they always match
//! what the tools produce. Each sample starts from the same small synthetic
//! app screen and adds the tool's real object.
use crate::{Annotation, Kind};
use capture_core::Document;
use image::{Rgba, RgbaImage};

const W: u32 = 480;
const H: u32 = 270;

fn fill(img: &mut RgbaImage, r: [u32; 4], c: [u8; 4]) {
    for y in r[1]..(r[1] + r[3]).min(img.height()) {
        for x in r[0]..(r[0] + r[2]).min(img.width()) {
            img.put_pixel(x, y, Rgba(c));
        }
    }
}
/// A plain app window: title bar, sidebar, text lines and an email row.
fn screen() -> RgbaImage {
    let mut img = RgbaImage::from_pixel(W, H, Rgba([236, 241, 243, 255]));
    fill(&mut img, [0, 0, W, 30], [45, 111, 134, 255]);
    fill(&mut img, [0, 30, 96, H - 30], [214, 223, 227, 255]);
    for i in 0..5 {
        fill(&mut img, [14, 48 + i * 26, 66, 10], [176, 190, 196, 255]);
    }
    for (i, w) in [300, 240, 270].iter().enumerate() {
        fill(
            &mut img,
            [120, 52 + i as u32 * 22, *w, 9],
            [184, 197, 202, 255],
        );
    }
    // The email row, which privacy samples hide.
    fill(&mut img, [120, 132, 210, 16], [92, 108, 115, 255]);
    fill(&mut img, [120, 166, 280, 9], [184, 197, 202, 255]);
    fill(&mut img, [120, 186, 120, 26], [24, 145, 193, 255]);
    img
}
fn obj(id: u64, kind: Kind, r: [f32; 4], color: [u8; 4], width: u32) -> Annotation {
    Annotation {
        focus: None,
        highlighter_rect: false,
        hard_edge: false,
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        rotation_degrees: 0.,
        mirror_x: false,
        group: None,
        id,
        kind,
        start: [r[0], r[1]],
        end: [r[0] + r[2], r[1] + r[3]],
        color,
        width,
        corner_radius: 0.,
        fill: None,
        points: vec![],
        text: None,
        callout: None,
    }
}
fn text_style(content: &str, size: f32, color: [u8; 4]) -> capture_core::text::TextStyle {
    capture_core::text::TextStyle {
        content: content.into(),
        size_points: size,
        bold: true,
        color: Some(color),
        horizontal: capture_core::text::HorizontalAlignment::Center,
        vertical: capture_core::text::VerticalAlignment::Middle,
        ..Default::default()
    }
}
fn render(objects: Vec<Annotation>) -> RgbaImage {
    let base = screen();
    let Ok(mut doc) = Document::from_image(base.clone()) else {
        return base;
    };
    doc.annotations = objects;
    doc.promote_features();
    doc.render().unwrap_or(base)
}
const BLUE: [u8; 4] = [24, 145, 193, 255];
const RED: [u8; 4] = [228, 70, 60, 255];
/// The picture for an article key, or `None` for an unknown key.
pub(super) fn draw(key: &str) -> Option<RgbaImage> {
    let email = [114., 126., 222., 28.];
    Some(match key {
        "blur" => render(vec![obj(1, Kind::Blur, email, [0; 4], 12)]),
        "pixelate" => render(vec![obj(1, Kind::Pixelate, email, [0; 4], 12)]),
        "redact" => render(vec![obj(1, Kind::Redact, email, [17, 17, 17, 255], 1)]),
        "arrow" => {
            let mut a = obj(1, Kind::Arrow, [380., 230., -200., -60.], RED, 5);
            a.end = [190., 172.];
            render(vec![a])
        }
        "shape" => {
            let mut r = obj(1, Kind::Rectangle, [112., 180., 136., 38.], RED, 4);
            r.corner_radius = 8.;
            let e = obj(2, Kind::Ellipse, [300., 40., 120., 60.], BLUE, 4);
            render(vec![r, e])
        }
        "text" => {
            let mut t = obj(1, Kind::Text, [250., 186., 200., 40.], [28, 36, 40, 255], 1);
            t.text = Some(text_style("Click Save", 22., [28, 36, 40, 255]));
            render(vec![t])
        }
        "callout" => {
            let mut c = obj(1, Kind::Callout, [280., 60., 170., 56.], BLUE, 2);
            let mut style = capture_core::callout::CalloutStyle::for_box(c.start, c.end, [W, H]);
            style.fill = [255, 255, 255, 255];
            style.outline = BLUE;
            style.pointer = [210., 196.];
            c.callout = Some(style);
            c.text = Some(text_style("Click Save", 20., [28, 36, 40, 255]));
            render(vec![c])
        }
        "steps" => {
            let mut v = vec![];
            for (i, p) in [[108., 57.], [108., 137.], [108., 199.]]
                .into_iter()
                .enumerate()
            {
                if let Ok(m) = capture_core::steps::marker(
                    i as u64 + 1,
                    p,
                    30,
                    BLUE,
                    (i as u32 + 1, false),
                    [W, H],
                ) {
                    v.push(m);
                }
            }
            render(v)
        }
        "penlighter" => {
            let mut h = obj(1, Kind::Rectangle, [116., 158., 292., 24.], [0; 4], 1);
            h.fill = Some([255, 222, 28, 120]);
            render(vec![h])
        }
        "focus" | "magnify" => {
            let mut f = obj(1, Kind::Rectangle, [112., 178., 140., 42.], [0; 4], 1);
            f.focus = Some(capture_core::focus::Focus::default());
            render(vec![f])
        }
        "crop" => {
            let mut r = obj(1, Kind::Rectangle, [104., 40., 340., 190.], BLUE, 3);
            r.line_style = None;
            render(vec![r])
        }
        "perspective" | "tilt" => {
            let base = screen();
            let Ok(mut doc) = Document::from_image(base.clone()) else {
                return Some(base);
            };
            doc.canvas_behavior.perspective = Some(capture_core::tilt::Tilt {
                quad: capture_core::bitmap::Perspective::TiltLeft.quad(0.55, 0.45),
                background: Some([30, 36, 39, 255]),
            });
            doc.promote_features();
            doc.render().unwrap_or(base)
        }
        "watermark" => {
            let mut img = screen();
            let _ = capture_core::watermark::apply(
                &mut img,
                Some(&capture_core::watermark::Watermark::default()),
            );
            img
        }
        "svg" => {
            let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="44" r="30" fill="#1891c1"/><path d="M36 44l10 10 20-22" stroke="#fff" stroke-width="7" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
            let mut img = RgbaImage::from_pixel(W, H, Rgba([244, 246, 247, 255]));
            for (size, x) in [(70u32, 70u32), (200, 230)] {
                if let Ok(p) = capture_core::vector_import::render_svg(svg, [size, size], None) {
                    image::imageops::overlay(&mut img, &p, x as i64, ((H - size) / 2) as i64);
                }
            }
            img
        }
        "template" => {
            let doc = capture_core::templates::build(
                "how-to",
                "How to export a report",
                &[],
                capture_core::templates::Scale::Fit,
            )
            .ok()?;
            let full = doc.render().ok()?;
            let thumb = image::imageops::thumbnail(&full, W, H);
            let mut img = RgbaImage::from_pixel(W, H, Rgba([214, 223, 227, 255]));
            image::imageops::overlay(&mut img, &thumb, ((W - thumb.width()) / 2) as i64, 0);
            img
        }
        "palette" => {
            let mut img = RgbaImage::from_pixel(W, H, Rgba([38, 44, 47, 255]));
            let colors = [
                [11, 61, 92, 255],
                [24, 145, 193, 255],
                [125, 214, 237, 255],
                [242, 201, 76, 255],
                [228, 135, 127, 255],
                [255, 255, 255, 255],
            ];
            for (i, c) in colors.iter().enumerate() {
                fill(&mut img, [40 + i as u32 * 68, 105, 56, 56], *c);
            }
            img
        }
        "timeline" => {
            // A video timeline: tracks of clips, a playhead, one clip flashing.
            let mut img = RgbaImage::from_pixel(W, H, Rgba([25, 31, 33, 255]));
            fill(&mut img, [0, 0, W, 24], [36, 43, 46, 255]);
            for t in 0..6 {
                fill(&mut img, [0, 30 + t * 38, W, 34], [31, 38, 41, 255]);
            }
            fill(&mut img, [20, 34, 420, 26], [45, 111, 134, 255]);
            fill(&mut img, [60, 72, 90, 26], [242, 201, 76, 255]);
            fill(&mut img, [190, 72, 120, 26], [74, 92, 99, 255]);
            fill(&mut img, [120, 110, 160, 26], [111, 84, 160, 255]);
            for x in 20..440u32 {
                let h =
                    (((x as f32 * 0.23).sin() * (x as f32 * 0.051).cos()).abs() * 22.) as u32 + 2;
                fill(&mut img, [x, 186 - h / 2, 1, h], [99, 197, 143, 255]);
            }
            fill(&mut img, [232, 24, 2, H - 24], [228, 70, 60, 255]);
            img
        }
        "webcam" => {
            let mut img = screen();
            let (cx, cy, r) = (410i32, 205i32, 46i32);
            for y in cy - r - 3..cy + r + 3 {
                for x in cx - r - 3..cx + r + 3 {
                    let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt();
                    if d <= r as f32 + 3. && x >= 0 && y >= 0 && (x as u32) < W && (y as u32) < H {
                        let c = if d > r as f32 {
                            [255, 255, 255, 255]
                        } else if d < 18. && y < cy {
                            [232, 196, 170, 255]
                        } else {
                            [70, 84, 92, 255]
                        };
                        img.put_pixel(x as u32, y as u32, Rgba(c));
                    }
                }
            }
            img
        }
        _ => return None,
    })
}
