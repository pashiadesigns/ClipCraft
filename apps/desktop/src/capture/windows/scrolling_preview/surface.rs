//! Antialiased, premultiplied surface for the layered Preview window.
//! The image stays at its cached resolution; only the small text bands use 4x GDI.
use super::*;
use vello_cpu::{
    Pixmap, RenderContext,
    color::{AlphaColor, Srgb},
    kurbo::{BezPath, Cap, Join, RoundedRect, Shape, Stroke},
};

fn finish(mut context: RenderContext, size: [u32; 2]) -> image::RgbaImage {
    let mut pixels = Pixmap::new(size[0] as u16, size[1] as u16);
    context.flush();
    context.render(&mut pixels, &mut vello_cpu::Resources::new());
    image::RgbaImage::from_raw(
        size[0],
        size[1],
        pixels
            .data()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect(),
    )
    .unwrap()
}
fn over(dst: &mut image::RgbaImage, src: &image::RgbaImage, left: u32, top: u32) {
    for (x, y, p) in src.enumerate_pixels() {
        if x + left >= dst.width() || y + top >= dst.height() {
            continue;
        }
        let d = dst.get_pixel_mut(x + left, y + top);
        for c in 0..4 {
            d[c] = (p[c] as u16 + (d[c] as u16 * (255 - p[3] as u16) + 127) / 255).min(255) as u8;
        }
    }
}
fn text_band(
    value: &str,
    width: u32,
    height: u32,
    dpi: f32,
    centered: bool,
) -> Result<image::RgbaImage, String> {
    let size = [width * 4, height * 4];
    unsafe {
        let dc = MemoryDc(CreateCompatibleDC(null_mut()));
        let mut bits = null_mut();
        let bitmap = Bitmap(CreateDIBSection(
            dc.0,
            &bitmap_info(size),
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        ));
        if bitmap.0.is_null() {
            return Err(win_error("Cannot draw Preview text"));
        }
        let old = SelectObject(dc.0, bitmap.0);
        std::ptr::write_bytes(bits.cast::<u8>(), 0, (size[0] * size[1] * 4) as usize);
        let font = Bitmap(CreateFontW(
            -(13. * dpi * 4.).round() as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32,
            w!("Segoe UI"),
        ));
        let f = SelectObject(dc.0, font.0);
        SetBkMode(dc.0, TRANSPARENT as i32);
        SetTextColor(dc.0, 0x00ffffff);
        let mut r = RECT {
            left: 0,
            top: 0,
            right: size[0] as i32,
            bottom: size[1] as i32,
        };
        let value: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        DrawTextW(
            dc.0,
            value.as_ptr(),
            -1,
            &mut r,
            if centered {
                DT_CENTER | DT_VCENTER | DT_SINGLELINE
            } else {
                DT_LEFT | DT_WORDBREAK | DT_END_ELLIPSIS
            },
        );
        GdiFlush();
        let bytes = std::slice::from_raw_parts(bits.cast::<u8>(), (size[0] * size[1] * 4) as usize);
        let mut rgba = image::RgbaImage::new(size[0], size[1]);
        for (out, b) in rgba.pixels_mut().zip(bytes.as_chunks::<4>().0.iter()) {
            let a = b[0].max(b[1]).max(b[2]);
            *out = image::Rgba([a, a, a, a]);
        }
        SelectObject(dc.0, f);
        SelectObject(dc.0, old);
        Ok(image::imageops::resize(
            &rgba,
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        ))
    }
}
pub(super) fn render(size: [u32; 2], s: &State) -> Result<image::RgbaImage, String> {
    let dpi = s.dpi as f64;
    let px = |v: f64| (v * dpi).round() as u32;
    let mut ctx = RenderContext::new(size[0] as u16, size[1] as u16);
    let outline = RoundedRect::new(
        dpi,
        dpi,
        size[0] as f64 - dpi,
        size[1] as f64 - dpi,
        8. * dpi,
    );
    ctx.set_paint(AlphaColor::<Srgb>::from_rgba8(43, 48, 51, 255));
    ctx.fill_path(&outline.to_path(0.1));
    ctx.set_paint(AlphaColor::<Srgb>::from_rgba8(25, 154, 195, 255));
    ctx.set_stroke(Stroke::new(1.5 * dpi));
    ctx.stroke_path(&outline.to_path(0.1));
    let mut out = finish(ctx, size);
    let header = text_band(
        "Preview",
        size[0].saturating_sub(px(54.)).max(1),
        px(24.),
        s.dpi,
        false,
    )?;
    over(&mut out, &header, px(12.), px(9.));
    if !s.pixels.is_empty() {
        let left = (size[0] - s.size[0]) / 2;
        for (i, p) in s.pixels.as_chunks::<4>().0.iter().enumerate() {
            let x = left + i as u32 % s.size[0];
            let y = px(36.) + i as u32 / s.size[0];
            if y >= out.height() {
                break;
            }
            let d = out.get_pixel_mut(x, y);
            for (c, source) in [p[2], p[1], p[0]].iter().enumerate() {
                d[c] = ((*source as u32 * p[3] as u32 + d[c] as u32 * (255 - p[3] as u32) + 127)
                    / 255) as u8;
            }
            d[3] = 255;
        }
        let r = expand_rect(size, s);
        let side = (r.right - r.left) as u32;
        let mut icon = RenderContext::new(side as u16, side as u16);
        icon.set_paint(AlphaColor::<Srgb>::from_rgba8(25, 154, 195, 255));
        icon.fill_path(&RoundedRect::new(0., 0., side as f64, side as f64, 5. * dpi).to_path(0.1));
        let mut path = BezPath::new();
        // Four independent, round-capped corner marks; no intersecting diagonals.
        for (x, y, sx, sy) in [
            (8., 8., 1., 1.),
            (22., 8., -1., 1.),
            (8., 22., 1., -1.),
            (22., 22., -1., -1.),
        ] {
            let (x, y) = if s.expanded {
                (x + sx * 4., y + sy * 4.)
            } else {
                (x, y)
            };
            let dir = if s.expanded { -1. } else { 1. };
            path.move_to(((x + sx * 5. * dir) * dpi, y * dpi));
            path.line_to((x * dpi, y * dpi));
            path.line_to((x * dpi, (y + sy * 5. * dir) * dpi));
        }
        icon.set_paint(AlphaColor::<Srgb>::WHITE);
        icon.set_stroke(
            Stroke::new(1.6 * dpi)
                .with_caps(Cap::Round)
                .with_join(Join::Round),
        );
        icon.stroke_path(&path);
        over(
            &mut out,
            &finish(icon, [side, side]),
            r.left as u32,
            r.top as u32,
        );
    } else {
        let waiting = text_band(
            "Waiting for capture",
            size[0] - px(16.),
            px(88.),
            s.dpi,
            true,
        )?;
        over(&mut out, &waiting, px(8.), px(36.));
    }
    let footer = px(if s.expanded { 40. } else { 72. });
    let status = text_band(
        &s.status,
        size[0].saturating_sub(px(20.)).max(1),
        footer - px(8.),
        s.dpi,
        false,
    )?;
    over(&mut out, &status, px(10.), size[1] - footer + px(4.));
    Ok(out)
}
