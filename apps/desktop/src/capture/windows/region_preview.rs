//! Region-only precision feedback. Samples the retained capture, never the overlay.
use super::*;

const BLUE: u32 = 0x00e2aa00; // COLORREF: ClipDesigner #00aae2
const SAMPLES: i32 = 21;

struct Layout {
    card: RECT,
    lens: RECT,
    zoom: i32,
}

impl Layout {
    fn new(point: [i32; 2], size: [i32; 2], dpi: u32) -> Self {
        let scale = dpi.clamp(96, 288) as i32;
        let pad = 6 * scale / 96;
        let footer = 24 * scale / 96;
        let gap = 12 * scale / 96;
        let beside = (point[0].max(size[0] - point[0]) - gap - 2 * pad - 4) / SAMPLES;
        let above_below = (point[1].max(size[1] - point[1]) - gap - 2 * pad - footer - 4) / SAMPLES;
        let zoom = (5 * scale / 96)
            .min((size[0] - 2 * pad - 8) / SAMPLES)
            .min((size[1] - 2 * pad - footer - 8) / SAMPLES)
            .min(beside.max(above_below))
            .max(1);
        let diameter = SAMPLES * zoom;
        let width = diameter + 2 * pad;
        let height = diameter + 2 * pad + footer;
        let left = if point[0] - gap - width >= 4 {
            point[0] - gap - width
        } else {
            point[0] + gap
        }
        .clamp(4, (size[0] - width - 4).max(4));
        let top = if point[1] + gap + height <= size[1] - 4 {
            point[1] + gap
        } else {
            point[1] - gap - height
        }
        .clamp(4, (size[1] - height - 4).max(4));
        Self {
            card: RECT {
                left,
                top,
                right: left + width,
                bottom: top + height,
            },
            lens: RECT {
                left: left + pad,
                top: top + pad,
                right: left + pad + diameter,
                bottom: top + pad + diameter,
            },
            zoom,
        }
    }
}

fn lens_pixels(snapshot: &Snapshot, pointer: [i32; 2], zoom: i32) -> Vec<u8> {
    let side = SAMPLES * zoom;
    let local = [
        pointer[0] - snapshot.origin[0],
        pointer[1] - snapshot.origin[1],
    ];
    let mut pixels = vec![0; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let sx = (local[0] + x / zoom - SAMPLES / 2).clamp(0, snapshot.size[0] as i32 - 1);
            let sy = (local[1] + y / zoom - SAMPLES / 2).clamp(0, snapshot.size[1] as i32 - 1);
            let source = (sy as usize * snapshot.size[0] as usize + sx as usize) * 4;
            let target = ((y * side + x) * 4) as usize;
            pixels[target..target + 4].copy_from_slice(&snapshot.pixels[source..source + 4]);
        }
    }
    pixels
}

pub(super) fn dashed(dc: HDC, points: &[[i32; 2]]) {
    unsafe {
        let saved = SaveDC(dc);
        SetBkMode(dc, TRANSPARENT as i32);
        // A one-pixel dark shadow makes the cyan dashes readable on light pixels.
        for (offset, color) in [(1, 0x00141414), (0, BLUE)] {
            let pen = Bitmap(CreatePen(PS_DASH, 1, color));
            let old = SelectObject(dc, pen.0);
            MoveToEx(dc, points[0][0] + offset, points[0][1] + offset, null_mut());
            for point in &points[1..] {
                LineTo(dc, point[0] + offset, point[1] + offset);
            }
            SelectObject(dc, old);
        }
        RestoreDC(dc, saved);
    }
}

fn label(dc: HDC, text: &str, mut rect: RECT, dpi: u32) {
    unsafe {
        let font = Bitmap(CreateFontW(
            -(13 * dpi.clamp(96, 288) as i32 / 96),
            0,
            0,
            0,
            500,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            w!("Segoe UI"),
        ));
        let old = SelectObject(dc, font.0);
        let mut text: Vec<u16> = text.encode_utf16().collect();
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00ffffff);
        DrawTextW(
            dc,
            text.as_mut_ptr(),
            text.len() as i32,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(dc, old);
    }
}

pub(super) fn paint(dc: HDC, picker: &Picker, origin: [i32; 2], size: [i32; 2], dpi: u32) {
    let point = [picker.pointer[0] - origin[0], picker.pointer[1] - origin[1]];
    if (0..size[1]).contains(&point[1]) {
        dashed(dc, &[[0, point[1]], [size[0], point[1]]]);
    }
    if (0..size[0]).contains(&point[0]) {
        dashed(dc, &[[point[0], 0], [point[0], size[1]]]);
    }
    // Only the monitor containing the pointer gets a lens. Other monitor
    // windows still paint intersecting guides in physical desktop coordinates.
    if !(0..size[0]).contains(&point[0]) || !(0..size[1]).contains(&point[1]) {
        return;
    }
    let layout = Layout::new(point, size, dpi);
    let side = SAMPLES * layout.zoom;
    let pixels = lens_pixels(&picker.snapshot, picker.pointer, layout.zoom);
    unsafe {
        let saved = SaveDC(dc);
        let brush = Bitmap(CreateSolidBrush(0x00252221));
        let pen = Bitmap(CreatePen(PS_SOLID, 1, 0x00736a62));
        SelectObject(dc, brush.0);
        SelectObject(dc, pen.0);
        let hint_width = (380 * dpi.clamp(96, 288) as i32 / 96).min(size[0] - 16);
        let hint = RECT {
            left: (size[0] - hint_width) / 2,
            top: 8,
            right: (size[0] + hint_width) / 2,
            bottom: 8 + 28 * dpi.clamp(96, 288) as i32 / 96,
        };
        RoundRect(dc, hint.left, hint.top, hint.right, hint.bottom, 8, 8);
        label(
            dc,
            if picker.start.is_some() {
                "Release to capture · Esc / right-click to cancel"
            } else {
                "Drag to capture · Esc / right-click to cancel"
            },
            hint,
            dpi,
        );
        let r = layout.card;
        RoundRect(dc, r.left, r.top, r.right, r.bottom, 8, 8);
        let r = layout.lens;
        let circle = Bitmap(CreateEllipticRgn(r.left, r.top, r.right, r.bottom));
        let clipped = SaveDC(dc);
        ExtSelectClipRgn(dc, circle.0, RGN_AND);
        StretchDIBits(
            dc,
            r.left,
            r.top,
            side,
            side,
            0,
            0,
            side,
            side,
            pixels.as_ptr().cast(),
            &bitmap_info([side as u32; 2]),
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        let cx = r.left + side / 2;
        let cy = r.top + side / 2;
        // Leave the target pixel unobscured; frame its exact enlarged cell.
        let cell_left = r.left + (SAMPLES / 2) * layout.zoom;
        let cell_top = r.top + (SAMPLES / 2) * layout.zoom;
        let cursor_pen = Bitmap(CreatePen(PS_SOLID, 1, BLUE));
        let old_pen = SelectObject(dc, cursor_pen.0);
        for [a, b] in [
            [[r.left, cy], [cell_left - 2, cy]],
            [[cell_left + layout.zoom + 2, cy], [r.right, cy]],
            [[cx, r.top], [cx, cell_top - 2]],
            [[cx, cell_top + layout.zoom + 2], [cx, r.bottom]],
        ] {
            MoveToEx(dc, a[0], a[1], null_mut());
            LineTo(dc, b[0], b[1]);
        }
        let cell_brush = Bitmap(CreateSolidBrush(BLUE));
        FrameRect(
            dc,
            &RECT {
                left: cell_left - 1,
                top: cell_top - 1,
                right: cell_left + layout.zoom + 1,
                bottom: cell_top + layout.zoom + 1,
            },
            cell_brush.0,
        );
        SelectObject(dc, old_pen);
        RestoreDC(dc, clipped);
        let text = picker.region().map_or_else(
            || format!("{}, {}", picker.pointer[0], picker.pointer[1]),
            |r| format!("{} × {}", r.width, r.height),
        );
        label(
            dc,
            &text,
            RECT {
                left: layout.card.left,
                top: layout.lens.bottom,
                right: layout.card.right,
                bottom: layout.card.bottom,
            },
            dpi,
        );
        RestoreDC(dc, saved);
    }
}

#[cfg(test)]
mod tests;
