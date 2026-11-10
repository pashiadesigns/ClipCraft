//! Tool cursors as real Windows cursors. Windows moves these itself between
//! app frames, so the crosshair and its tool badge never trail the pointer the
//! way paint inside the canvas does. Images are drawn once and cached.
use crate::egui;
use std::{cell::RefCell, collections::HashMap};

/// Larger cursors fall back to the painted mark.
pub(crate) const MAX_SIDE: u32 = 256;

pub(crate) struct Image {
    pub size: [u32; 2],
    pub hotspot: [u32; 2],
    /// Premultiplied RGBA, top row first.
    pub pixels: Vec<u8>,
}
struct Painter {
    ctx: egui::Context,
    textures: HashMap<egui::TextureId, ([usize; 2], Vec<egui::Color32>)>,
}
thread_local! {
    static PAINTER: RefCell<Option<Painter>> = const { RefCell::new(None) };
}

/// Draws egui shapes into a `side`-pixel image, with the pointer at the center.
pub(crate) fn rasterize(side: u32, paint: impl FnOnce(&egui::Painter, egui::Pos2)) -> Image {
    let mut paint = Some(paint);
    let center = side / 2;
    PAINTER.with_borrow_mut(|slot| {
        let state = slot.get_or_insert_with(|| Painter {
            ctx: egui::Context::default(),
            textures: HashMap::new(),
        });
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(side as f32));
        let mut output = state.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(area),
                ..Default::default()
            },
            |ui| {
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::unique("native-cursor"),
                ));
                if let Some(paint) = paint.take() {
                    paint(&painter, egui::pos2(center as f32, center as f32));
                }
            },
        );
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                let egui::ImageData::Color(image) = &delta.image;
                match delta.pos {
                    None => {
                        state
                            .textures
                            .insert(*id, (image.size, image.pixels.clone()));
                    }
                    Some([x, y]) => {
                        if let Some((size, pixels)) = state.textures.get_mut(id) {
                            for row in 0..image.size[1] {
                                let from = row * image.size[0];
                                let to = (y + row) * size[0] + x;
                                pixels[to..to + image.size[0]]
                                    .copy_from_slice(&image.pixels[from..from + image.size[0]]);
                            }
                        }
                    }
                }
            }
        }
        for id in &output.textures_delta.free {
            state.textures.remove(id);
        }
        output.textures_delta.clear();
        let mut target = vec![0f32; side as usize * side as usize * 4];
        for shape in state.ctx.tessellate(output.shapes, 1.) {
            if let egui::epaint::Primitive::Mesh(mesh) = shape.primitive {
                draw_mesh(
                    &mut target,
                    side,
                    &mesh,
                    state.textures.get(&mesh.texture_id),
                );
            }
        }
        Image {
            size: [side, side],
            hotspot: [center, center],
            pixels: target
                .iter()
                .map(|v| (v * 255.).round().clamp(0., 255.) as u8)
                .collect(),
        }
    })
}

fn draw_mesh(
    target: &mut [f32],
    side: u32,
    mesh: &egui::Mesh,
    texture: Option<&([usize; 2], Vec<egui::Color32>)>,
) {
    let edge = |a: egui::Pos2, b: egui::Pos2, x: f32, y: f32| {
        (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
    };
    for tri in mesh.indices.as_chunks::<3>().0 {
        let [v0, v1, v2] = tri.map(|i| mesh.vertices[i as usize]);
        let area = edge(v0.pos, v1.pos, v2.pos.x, v2.pos.y);
        if area == 0. {
            continue;
        }
        let low = v0.pos.min(v1.pos).min(v2.pos);
        let high = v0.pos.max(v1.pos).max(v2.pos);
        let (left, top) = (low.x.floor().max(0.) as u32, low.y.floor().max(0.) as u32);
        let (right, bottom) = (
            (high.x.ceil().max(0.) as u32).min(side),
            (high.y.ceil().max(0.) as u32).min(side),
        );
        for y in top..bottom {
            for x in left..right {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w = [
                    edge(v1.pos, v2.pos, px, py) / area,
                    edge(v2.pos, v0.pos, px, py) / area,
                    edge(v0.pos, v1.pos, px, py) / area,
                ];
                if w.iter().any(|n| *n < 0.) {
                    continue;
                }
                let u = v0.uv.x * w[0] + v1.uv.x * w[1] + v2.uv.x * w[2];
                let v = v0.uv.y * w[0] + v1.uv.y * w[1] + v2.uv.y * w[2];
                let tex = texture.map_or([255; 4], |([tw, th], pixels)| {
                    let sx = ((u * *tw as f32) as usize).min(tw - 1);
                    let sy = ((v * *th as f32) as usize).min(th - 1);
                    pixels[sy * tw + sx].to_array()
                });
                let at = (y as usize * side as usize + x as usize) * 4;
                let mut src = [0f32; 4];
                for c in 0..4 {
                    let ink = f32::from(v0.color.to_array()[c]) * w[0]
                        + f32::from(v1.color.to_array()[c]) * w[1]
                        + f32::from(v2.color.to_array()[c]) * w[2];
                    src[c] = ink / 255. * f32::from(tex[c]) / 255.;
                }
                for c in 0..4 {
                    target[at + c] = src[c] + target[at + c] * (1. - src[3]);
                }
            }
        }
    }
}

#[cfg(all(windows, not(test)))]
mod platform {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use windows_sys::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};

    static ACTIVE: AtomicIsize = AtomicIsize::new(0);
    static REQUESTED: AtomicBool = AtomicBool::new(false);
    thread_local! {
        static CACHE: RefCell<HashMap<String, isize>> = RefCell::new(HashMap::new());
    }
    const CACHE_LIMIT: usize = 128;

    unsafe fn create(image: &Image) -> HCURSOR {
        let [w, h] = image.size.map(|n| n as i32);
        unsafe {
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = std::ptr::null_mut();
            let dc = GetDC(std::ptr::null_mut());
            let color = CreateDIBSection(
                dc,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            ReleaseDC(std::ptr::null_mut(), dc);
            if color.is_null() || bits.is_null() {
                return std::ptr::null_mut();
            }
            let out = std::slice::from_raw_parts_mut(bits as *mut u8, image.pixels.len());
            for (to, from) in out
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(image.pixels.as_chunks::<4>().0)
            {
                to.copy_from_slice(&[from[2], from[1], from[0], from[3]]);
            }
            let zero = vec![0u8; (w as usize).div_ceil(16) * 2 * h as usize];
            let mask = CreateBitmap(w, h, 1, 1, zero.as_ptr() as _);
            let icon = ICONINFO {
                fIcon: 0,
                xHotspot: image.hotspot[0],
                yHotspot: image.hotspot[1],
                hbmMask: mask,
                hbmColor: color,
            };
            let cursor = CreateIconIndirect(&icon);
            DeleteObject(color as _);
            DeleteObject(mask as _);
            cursor
        }
    }

    pub(crate) fn show(key: &str, build: impl FnOnce() -> Option<Image>) -> bool {
        CACHE.with_borrow_mut(|cache| {
            let cursor = match cache.get(key) {
                Some(c) => *c,
                None => {
                    let Some(image) = build() else {
                        return false;
                    };
                    if image.size.iter().any(|n| *n == 0 || *n > MAX_SIDE) {
                        return false;
                    }
                    let made = unsafe { create(&image) } as isize;
                    if made == 0 {
                        return false;
                    }
                    if cache.len() >= CACHE_LIMIT {
                        let active = ACTIVE.load(Ordering::Relaxed);
                        cache.retain(|_, c| {
                            if *c == active {
                                return true;
                            }
                            unsafe { DestroyCursor(*c as HCURSOR) };
                            false
                        });
                    }
                    cache.insert(key.to_owned(), made);
                    made
                }
            };
            if ACTIVE.swap(cursor, Ordering::Relaxed) != cursor {
                unsafe { SetCursor(cursor as HCURSOR) };
            }
            REQUESTED.store(true, Ordering::Relaxed);
            true
        })
    }

    /// Called at the start of each frame: a frame that asked for no tool
    /// cursor hands the pointer back to the normal cursors.
    pub(crate) fn next_frame() {
        if !REQUESTED.swap(false, Ordering::Relaxed) {
            ACTIVE.store(0, Ordering::Relaxed);
        }
    }

    /// `WM_SETCURSOR`: true when the tool cursor was applied.
    pub(crate) fn apply(lparam: isize) -> bool {
        let cursor = ACTIVE.load(Ordering::Relaxed);
        if cursor == 0 || (lparam & 0xffff) as u32 != HTCLIENT {
            return false;
        }
        unsafe { SetCursor(cursor as HCURSOR) };
        true
    }
}
#[cfg(all(windows, not(test)))]
pub(crate) use platform::*;

/// Real open and closed hands (approved `artifacts/br-proposal/bend-cursor.html`).
/// Windows draws every hand egui asks for as the four-arrow move cursor, so a
/// frame that ends asking for one shows ClipCraft's own instead.
pub(crate) fn install_hands(ctx: &egui::Context) {
    let installed = egui::Id::unique("clipcraft-hands-installed");
    if ctx.data(|d| d.get_temp::<bool>(installed)).is_some() {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(installed, true));
    ctx.on_end_pass(
        "clipcraft-hands",
        std::sync::Arc::new(|ui| {
            let closed = match ui.ctx().output(|o| o.cursor_icon) {
                egui::CursorIcon::Grab => false,
                egui::CursorIcon::Grabbing => true,
                _ => return,
            };
            show(if closed { "hand-closed" } else { "hand-open" }, || {
                Some(rasterize(32, |p, pos| paint_hand(p, pos, closed)))
            });
        }),
    );
}
/// A hand drawn around `pos`, its hotspot on the palm.
pub(crate) fn paint_hand(p: &egui::Painter, pos: egui::Pos2, closed: bool) {
    let at = |x: f32, y: f32| pos + egui::vec2(x, y);
    let pieces: Vec<egui::Rect> = if closed {
        vec![
            egui::Rect::from_min_max(at(-6., -3.), at(7., 9.)),
            egui::Rect::from_min_max(at(-6., -6.), at(-2., 0.)),
            egui::Rect::from_min_max(at(-2., -7.), at(2., 0.)),
            egui::Rect::from_min_max(at(2., -7.), at(6., 0.)),
            egui::Rect::from_min_max(at(5., -5.), at(9., 2.)),
        ]
    } else {
        vec![
            egui::Rect::from_min_max(at(-6., -1.), at(7., 9.)),
            egui::Rect::from_min_max(at(-6., -10.), at(-3., 1.)),
            egui::Rect::from_min_max(at(-2.5, -12.), at(0.5, 1.)),
            egui::Rect::from_min_max(at(1., -11.), at(4., 1.)),
            egui::Rect::from_min_max(at(4.5, -8.), at(7.5, 1.)),
            egui::Rect::from_min_max(at(-10., -1.), at(-5., 3.)),
        ]
    };
    let ink = egui::Color32::from_gray(20);
    for r in &pieces {
        p.rect_filled(r.expand(1.2), 2.5, ink);
    }
    for r in &pieces {
        p.rect_filled(*r, 1.8, egui::Color32::WHITE);
    }
}

#[cfg(not(all(windows, not(test))))]
pub(crate) fn show(_key: &str, _build: impl FnOnce() -> Option<Image>) -> bool {
    false
}
#[cfg(not(all(windows, not(test))))]
pub(crate) fn next_frame() {}
#[cfg(all(windows, test))]
pub(crate) fn apply(_lparam: isize) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hands_are_white_with_a_dark_outline_and_differ_open_and_closed() {
        let open = rasterize(32, |p, pos| paint_hand(p, pos, false));
        let closed = rasterize(32, |p, pos| paint_hand(p, pos, true));
        let at = |i: &Image, x: usize, y: usize| {
            i.pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].to_vec()
        };
        assert_eq!(at(&open, 16, 20), vec![255; 4], "the palm is white");
        assert_eq!(at(&open, 15, 6)[3], 255, "an open hand has long fingers");
        assert_eq!(at(&closed, 15, 6)[3], 0, "a closed hand doesn't");
        assert_eq!(at(&open, 1, 1)[3], 0);
    }
    #[test]
    fn cursor_images_hold_the_mark_with_its_hotspot_at_the_center() {
        let image = rasterize(48, |p, pos| {
            p.circle_filled(pos, 6., egui::Color32::from_rgb(200, 40, 20));
            p.text(
                pos + egui::vec2(14., 12.),
                egui::Align2::CENTER_CENTER,
                "7",
                egui::FontId::proportional(12.),
                egui::Color32::WHITE,
            );
        });
        assert_eq!(image.size, [48, 48]);
        assert_eq!(image.hotspot, [24, 24]);
        let at = |x: usize, y: usize| &image.pixels[(y * 48 + x) * 4..(y * 48 + x) * 4 + 4];
        assert_eq!(at(24, 24)[3], 255, "the mark is at the hotspot");
        assert!(at(24, 24)[0] > 190 && at(24, 24)[2] < 30);
        assert_eq!(at(2, 2)[3], 0, "the corners stay clear");
        let glyph = (30..46)
            .flat_map(|y| (30..46).map(move |x| (x, y)))
            .filter(|(x, y)| at(*x, *y)[3] > 120)
            .count();
        assert!(glyph > 4, "text comes from the font atlas: {glyph}");
        for p in image.pixels.as_chunks::<4>().0 {
            assert!(
                p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3],
                "premultiplied"
            );
        }
    }
}
