//! SVG and EPS import (EDIT-084, approved bi-proposal 6).
//!
//! SVG stays a drawing: the image object keeps the SVG text and redraws it
//! at whatever size it is shown or exported, so it is sharp at any zoom.
//! For safety an SVG never loads web links or other files; only images
//! embedded in the file itself are drawn.
//!
//! EPS is PostScript, which no Rust library can draw. When Ghostscript is
//! installed, ClipCraft asks it to draw the file at high resolution; it is
//! not bundled (its AGPL license doesn't fit Apache-2.0). Without it,
//! ClipCraft uses the preview image stored inside the EPS, and says so.
use crate::{Error, Result};
use image::RgbaImage;
use resvg::{tiny_skia, usvg};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

const MAX_SVG_BYTES: usize = 8 * 1024 * 1024;
/// Longest side ClipCraft draws an SVG at, for memory.
pub const MAX_SIDE: u32 = 4096;

fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}
fn options() -> usvg::Options<'static> {
    let mut opt = usvg::Options {
        resources_dir: None,
        fontdb: fonts(),
        ..Default::default()
    };
    // Embedded (data:) images only: never read another file or a link.
    opt.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
        resolve_string: Box::new(|_, _| None),
    };
    opt
}
fn parse(text: &str) -> Result<usvg::Tree> {
    if text.len() > MAX_SVG_BYTES {
        return Err(Error::Invalid("SVG files must be under 8 MB.".into()));
    }
    usvg::Tree::from_str(text, &options())
        .map_err(|e| Error::Invalid(format!("This SVG can't be read: {e}")))
}
/// The drawing's own size in pixels, kept within sensible bounds.
pub fn svg_size(text: &str) -> Result<[u32; 2]> {
    let size = parse(text)?.size();
    let (w, h) = (size.width().max(1.), size.height().max(1.));
    let fit = (2048. / w.max(h))
        .min(1.)
        .max(64. / w.min(h))
        .min(MAX_SIDE as f32 / w.max(h));
    Ok([
        ((w * fit).round() as u32).max(1),
        ((h * fit).round() as u32).max(1),
    ])
}
/// Draws an SVG at `size` pixels, optionally in one color (`tint`).
pub fn render_svg(text: &str, size: [u32; 2], tint: Option<[u8; 4]>) -> Result<RgbaImage> {
    let tree = parse(text)?;
    let [w, h] = size.map(|v| v.clamp(1, MAX_SIDE));
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| Error::Invalid("SVG size is out of range.".into()))?;
    let s = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        w as f32 / s.width().max(1e-3),
        h as f32 / s.height().max(1e-3),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut out = RgbaImage::new(w, h);
    for (p, c) in out.pixels_mut().zip(pixmap.pixels()) {
        let c = c.demultiply();
        *p = image::Rgba(match tint {
            Some(t) => [
                t[0],
                t[1],
                t[2],
                ((u16::from(c.alpha()) * u16::from(t[3]) + 127) / 255) as u8,
            ],
            None => [c.red(), c.green(), c.blue(), c.alpha()],
        });
    }
    Ok(out)
}

/// Converts an SVG into an editable vector icon (BI11): its shapes become
/// icon paths in the 128-pixel icon box and its colors become up to five
/// editable color slots, like ClipCraft's own icons. SVGs that need more than
/// that (pictures, more than five colors, very many shapes) return an error
/// and stay drawings, which can still be recolored as a whole.
pub fn svg_vector_icon(text: &str) -> Result<crate::icon_library::vector::VectorIcon> {
    use crate::icon_library::vector::{Path as IconPath, VectorIcon};
    let tree = parse(text)?;
    let size = tree.size();
    let (w, h) = (size.width().max(1e-3), size.height().max(1e-3));
    let scale = 120. / w.max(h);
    let offset = [(128. - w * scale) / 2., (128. - h * scale) / 2.];
    let place = tiny_skia::Transform::from_row(scale, 0., 0., scale, offset[0], offset[1]);
    // (color, stroke width or None for fill, path data)
    let mut shapes: Vec<([u8; 4], Option<f64>, String)> = vec![];
    fn paint_color(paint: &usvg::Paint, opacity: f32) -> Option<[u8; 4]> {
        let (c, o) = match paint {
            usvg::Paint::Color(c) => (*c, 1.),
            usvg::Paint::LinearGradient(g) => {
                let s = g.stops().first()?;
                (s.color(), s.opacity().get())
            }
            usvg::Paint::RadialGradient(g) => {
                let s = g.stops().first()?;
                (s.color(), s.opacity().get())
            }
            usvg::Paint::Pattern(_) => return None,
        };
        Some([
            c.red,
            c.green,
            c.blue,
            (o * opacity * 255.).round().clamp(0., 255.) as u8,
        ])
    }
    fn data(path: &tiny_skia::Path) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let n = |v: f32| (v * 100.).round() / 100.;
        for segment in path.segments() {
            let _ = match segment {
                tiny_skia::PathSegment::MoveTo(p) => write!(out, "M {} {} ", n(p.x), n(p.y)),
                tiny_skia::PathSegment::LineTo(p) => write!(out, "L {} {} ", n(p.x), n(p.y)),
                tiny_skia::PathSegment::QuadTo(a, p) => {
                    write!(out, "Q {} {} {} {} ", n(a.x), n(a.y), n(p.x), n(p.y))
                }
                tiny_skia::PathSegment::CubicTo(a, b, p) => write!(
                    out,
                    "C {} {} {} {} {} {} ",
                    n(a.x),
                    n(a.y),
                    n(b.x),
                    n(b.y),
                    n(p.x),
                    n(p.y)
                ),
                tiny_skia::PathSegment::Close => write!(out, "Z "),
            };
        }
        out.trim_end().to_owned()
    }
    fn walk(
        group: &usvg::Group,
        place: tiny_skia::Transform,
        shapes: &mut Vec<([u8; 4], Option<f64>, String)>,
    ) -> Result<()> {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => walk(g, place, shapes)?,
                usvg::Node::Text(t) => walk(t.flattened(), place, shapes)?,
                usvg::Node::Image(_) => {
                    return Err(Error::Invalid("The SVG contains pictures.".into()));
                }
                usvg::Node::Path(p) => {
                    if !p.is_visible() {
                        continue;
                    }
                    let ts = p.abs_transform().post_concat(place);
                    let Some(geometry) = p.data().clone().transform(ts) else {
                        continue;
                    };
                    let d = data(&geometry);
                    if d.is_empty() {
                        continue;
                    }
                    // A straight line has nothing to fill (SVG's default black
                    // fill on it is invisible and must not take a color slot).
                    let bounds = geometry.bounds();
                    let area = bounds.width() > 0.01 && bounds.height() > 0.01;
                    if let Some(fill) = p.fill().filter(|_| area) {
                        let color = paint_color(fill.paint(), fill.opacity().get())
                            .ok_or_else(|| Error::Invalid("The SVG uses a pattern fill.".into()))?;
                        shapes.push((color, None, d.clone()));
                    }
                    if let Some(stroke) = p.stroke() {
                        let color = paint_color(stroke.paint(), stroke.opacity().get())
                            .ok_or_else(|| {
                                Error::Invalid("The SVG uses a pattern stroke.".into())
                            })?;
                        let (sx, sy) = (ts.sx.hypot(ts.ky), ts.kx.hypot(ts.sy));
                        let width = f64::from(stroke.width().get() * (sx + sy) / 2.);
                        shapes.push((color, Some(width.clamp(0.01, 128.)), d));
                    }
                }
            }
        }
        Ok(())
    }
    walk(tree.root(), place, &mut shapes)?;
    if shapes.is_empty() {
        return Err(Error::Invalid("The SVG has no shapes.".into()));
    }
    // Colors that differ only slightly share one slot.
    let mut colors: Vec<[u8; 4]> = vec![];
    let mut slot_of = |c: [u8; 4]| -> Option<usize> {
        if let Some(i) = colors.iter().position(|k| {
            k.iter()
                .zip(&c)
                .map(|(a, b)| a.abs_diff(*b) as u32)
                .sum::<u32>()
                <= 12
        }) {
            return Some(i);
        }
        (colors.len() < 5).then(|| {
            colors.push(c);
            colors.len() - 1
        })
    };
    let mut paths: Vec<IconPath> = vec![];
    for (color, stroke, d) in shapes {
        let slot = slot_of(color)
            .ok_or_else(|| Error::Invalid("The SVG has more than five colors.".into()))?;
        // Consecutive shapes with the same look join into one path.
        match paths.last_mut() {
            Some(last)
                if last.color == slot
                    && last.stroke == stroke
                    && last.data.len() + d.len() < 30_000 =>
            {
                last.data.push(' ');
                last.data.push_str(&d);
            }
            _ => paths.push(IconPath {
                data: d,
                color: slot,
                stroke,
            }),
        }
    }
    let icon = VectorIcon { colors, paths };
    icon.validate()?;
    Ok(icon)
}

/// Ghostscript's console program, when installed.
pub fn ghostscript() -> Option<PathBuf> {
    let names = ["gswin64c.exe", "gswin32c.exe", "gs.exe", "gs"];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    for root in ["C:\\Program Files\\gs", "C:\\Program Files (x86)\\gs"] {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut versions: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        versions.sort();
        for dir in versions.into_iter().rev() {
            for name in ["gswin64c.exe", "gswin32c.exe"] {
                let candidate = dir.join("bin").join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}
/// How an EPS came in, for the status message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpsSource {
    Ghostscript,
    Preview,
}
/// The PostScript part of a file (DOS EPS files wrap it with previews).
fn postscript(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) && bytes.len() >= 30 {
        let u = |i: usize| {
            u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize
        };
        let (start, len) = (u(4), u(8));
        if let Some(ps) = bytes.get(start..start.saturating_add(len)) {
            return ps;
        }
    }
    bytes
}
fn bounding_box(ps: &[u8]) -> Option<[f32; 4]> {
    let text = String::from_utf8_lossy(&ps[..ps.len().min(64 * 1024)]);
    text.lines()
        .find_map(|l| l.strip_prefix("%%BoundingBox:"))
        .and_then(|rest| {
            let v: Vec<f32> = rest
                .split_whitespace()
                .filter_map(|n| n.parse().ok())
                .collect();
            (v.len() == 4 && v[2] > v[0] && v[3] > v[1]).then(|| [v[0], v[1], v[2], v[3]])
        })
}
/// Reads an EPS file as an image: Ghostscript when installed, otherwise the
/// preview stored in the file.
pub fn eps_image(bytes: &[u8]) -> Result<(RgbaImage, EpsSource)> {
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(Error::Invalid("EPS files must be under 64 MB.".into()));
    }
    if let Some(gs) = ghostscript()
        && let Ok(image) = with_ghostscript(&gs, postscript(bytes))
    {
        return Ok((image, EpsSource::Ghostscript));
    }
    preview(bytes)
        .map(|image| (image, EpsSource::Preview))
        .ok_or_else(|| {
            Error::Invalid(
                "This EPS has no preview image. Install Ghostscript to import it, or save it as SVG."
                    .into(),
            )
        })
}
fn with_ghostscript(gs: &Path, ps: &[u8]) -> Result<RgbaImage> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("input.eps");
    let output = dir.path().join("output.png");
    std::fs::write(&input, ps)?;
    let [x0, y0, x1, y1] = bounding_box(ps).unwrap_or([0., 0., 612., 792.]);
    let dpi = (3000. * 72. / (x1 - x0).max(y1 - y0))
        .clamp(72., 1200.)
        .round();
    let mut command = std::process::Command::new(gs);
    command.args([
        "-dSAFER",
        "-dBATCH",
        "-dNOPAUSE",
        "-dQUIET",
        "-dEPSCrop",
        "-dTextAlphaBits=4",
        "-dGraphicsAlphaBits=4",
        "-sDEVICE=pngalpha",
        &format!("-r{dpi}"),
    ]);
    command
        .arg(format!("-sOutputFile={}", output.display()))
        .arg(&input);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(Error::Invalid("Ghostscript couldn't draw this EPS.".into()));
            }
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(30) {
            let _ = child.kill();
            return Err(Error::Invalid(
                "Ghostscript took too long to draw this EPS.".into(),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let image = image::open(&output)
        .map_err(|e| Error::Invalid(format!("Ghostscript's output couldn't be read: {e}")))?
        .to_rgba8();
    crate::validate_dimensions(image.width(), image.height())?;
    Ok(image)
}
/// The preview inside an EPS: a TIFF in DOS EPS files, or an EPSI bitmap.
fn preview(bytes: &[u8]) -> Option<RgbaImage> {
    if bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) && bytes.len() >= 30 {
        let u = |i: usize| {
            u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize
        };
        let (start, len) = (u(20), u(24));
        if len > 0
            && let Some(tiff) = bytes.get(start..start.saturating_add(len))
            && let Ok(image) = image::load_from_memory_with_format(tiff, image::ImageFormat::Tiff)
        {
            return Some(image.to_rgba8());
        }
    }
    epsi(postscript(bytes))
}
fn epsi(ps: &[u8]) -> Option<RgbaImage> {
    let text = String::from_utf8_lossy(ps);
    let mut lines = text.lines();
    let header = lines.find_map(|l| l.strip_prefix("%%BeginPreview:"))?;
    let v: Vec<u32> = header
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [w, h, depth] = [*v.first()?, *v.get(1)?, *v.get(2)?];
    if w == 0 || h == 0 || w > 4096 || h > 4096 || ![1, 2, 4, 8].contains(&depth) {
        return None;
    }
    let mut data = vec![];
    for line in lines {
        if line.starts_with("%%EndPreview") {
            break;
        }
        let hex = line.trim_start_matches('%').trim();
        let digits: Vec<u8> = hex
            .bytes()
            .filter_map(|b| (b as char).to_digit(16).map(|d| d as u8))
            .collect();
        data.extend(
            digits
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| c[0] << 4 | c[1]),
        );
    }
    let row = (w * depth).div_ceil(8) as usize;
    if data.len() < row * h as usize {
        return None;
    }
    let max = (1u32 << depth) - 1;
    Some(RgbaImage::from_fn(w, h, |x, y| {
        let bit = x * depth;
        let byte = data[y as usize * row + (bit / 8) as usize];
        let value = (u32::from(byte) >> (8 - depth - bit % 8)) & max;
        // EPSI: 0 is white and the largest value is black.
        let g = (255 - value * 255 / max) as u8;
        image::Rgba([g, g, g, 255])
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50">
        <rect x="10" y="10" width="80" height="30" fill="#1891c1"/>
        <image href="C:/Windows/win.ini" x="0" y="0" width="10" height="10"/>
        <image href="https://example.com/a.png" x="0" y="0" width="10" height="10"/>
    </svg>"##;
    #[test]
    fn svg_draws_sharp_at_any_size_without_loading_other_files() {
        assert_eq!(
            svg_size(SVG).unwrap(),
            [128, 64],
            "small drawings start at a usable size"
        );
        for scale in [1, 4] {
            let img = render_svg(SVG, [100 * scale, 50 * scale], None).unwrap();
            let edge = img.get_pixel(10 * scale, 25 * scale);
            assert_eq!(edge.0, [24, 145, 193, 255], "crisp edge at {scale}x");
            assert_eq!(img.get_pixel(1, 1)[3], 0, "outside links draw nothing");
        }
        let tinted = render_svg(SVG, [100, 50], Some([255, 0, 0, 255])).unwrap();
        assert_eq!(tinted.get_pixel(50, 25).0, [255, 0, 0, 255]);
        assert!(render_svg("<svg", [10, 10], None).is_err());
        assert!(parse(&"x".repeat(MAX_SVG_BYTES + 1)).is_err());
    }
    /// BI11: an SVG becomes a vector icon with its own color slots, which
    /// edit like ClipCraft's icons; SVGs it can't express stay drawings.
    #[test]
    fn svg_becomes_an_editable_vector_icon_when_it_fits() {
        let two = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 200 100">
            <g transform="translate(10 0)">
              <rect x="0" y="10" width="80" height="80" fill="#1891c1"/>
              <circle cx="140" cy="50" r="30" fill="#da313e" stroke="#1891c1" stroke-width="6"/>
              <path d="M 10 95 L 180 95" stroke="#1a91c2" stroke-width="4"/>
            </g>
        </svg>"##;
        let icon = svg_vector_icon(two).unwrap();
        assert_eq!(icon.colors.len(), 2, "near-identical blues share a slot");
        assert!(icon.colors.contains(&[24, 145, 193, 255]));
        assert!(
            icon.paths.iter().any(|p| p.stroke.is_some()),
            "strokes stay strokes"
        );
        // Recoloring a slot changes the drawing.
        let mut edited = icon.clone();
        let blue = edited
            .colors
            .iter()
            .position(|c| *c == [24, 145, 193, 255])
            .unwrap();
        edited.colors[blue] = [20, 160, 60, 255];
        let draw = |icon| {
            let a = Arc::new(crate::bitmap::Bitmap::from_vector(icon).unwrap()).annotation(1);
            crate::display::render_annotation_preview(
                &a,
                crate::display::Viewport {
                    size: [128, 128],
                    origin: [0.; 2],
                    scale: 1.,
                },
            )
            .unwrap()
        };
        let before = draw(icon);
        assert!(before.pixels().any(|p| p.0 == [24, 145, 193, 255]));
        assert_ne!(before, draw(edited));
        // Links to other files are dropped, so this one converts too.
        assert!(svg_vector_icon(SVG).is_ok());
        let many: String = (0..7)
            .map(|i| {
                format!(
                    r##"<rect x="{}" y="0" width="10" height="10" fill="#{:02x}3060"/>"##,
                    i * 12,
                    i * 30
                )
            })
            .collect();
        let many = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="90" height="10">{many}</svg>"#
        );
        assert!(
            svg_vector_icon(&many).is_err(),
            "more than five colors stays a drawing"
        );
        let picture = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==" width="10" height="10"/></svg>"#;
        assert!(svg_vector_icon(picture).is_err(), "pictures stay drawings");
    }
    #[test]
    fn eps_preview_is_used_without_ghostscript() {
        // A 4×2 EPSI preview: the first row black, the second white.
        let eps = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 4 2\n%%BeginPreview: 4 2 1 2\n% F0\n% 00\n%%EndPreview\nshowpage\n";
        let image = preview(eps.as_bytes()).unwrap();
        assert_eq!(image.dimensions(), (4, 2));
        assert_eq!(image.get_pixel(0, 0)[0], 0);
        assert_eq!(image.get_pixel(0, 1)[0], 255);
        assert_eq!(bounding_box(eps.as_bytes()), Some([0., 0., 4., 2.]));
        assert!(preview(b"%!PS-Adobe-3.0 EPSF-3.0\nshowpage\n").is_none());
    }
}
