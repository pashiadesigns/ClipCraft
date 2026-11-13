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
            ACTIVE.store(cursor, Ordering::Relaxed);
            // Also restores the tool cursor after winit set its own when the
            // egui cursor changed on the previous frame.
            if unsafe { GetCursor() } as isize != cursor {
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

    /// Drops the tool cursor at once (tool change, Escape), so a cursor from the
    /// previous tool can never outlive it until the next repaint (beta 4 retest).
    pub(crate) fn reset() {
        ACTIVE.store(0, Ordering::Relaxed);
        REQUESTED.store(false, Ordering::Relaxed);
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
/// Only rotation handles and the middle curve node of a line or arrow use them
/// (BT, user review of BS7/BS8); everything else keeps its usual cursor.
const HAND_REQUEST: &str = "clipdesigner-hand-request";
/// The egui cursor while a hand shows. It must stay visible: `None` makes winit
/// call `ShowCursor(FALSE)`, which hid the hand too (BT1). It is also the same
/// for open and closed, because winit sets its own cursor whenever egui's
/// changes and Windows sends no `WM_SETCURSOR` while a button is held.
pub(crate) const HAND_HOST: egui::CursorIcon = egui::CursorIcon::Default;
/// What a grip cursor offers (approved option D, `artifacts/beta5-proposal`):
/// a curved double arrow on rotation handles and a pointing finger on the
/// middle node of a line or arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grip {
    Rotate,
    Bend,
}
/// Asks for a grip cursor for this frame; `held` while its handle is dragged.
pub(crate) fn request_grip(ctx: &egui::Context, grip: Grip, held: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(HAND_REQUEST), (grip, held)));
}
/// The grip asked for this frame, if any (read by the end of the frame).
pub(crate) fn requested_grip(ctx: &egui::Context) -> Option<(Grip, bool)> {
    ctx.data(|d| d.get_temp::<(Grip, bool)>(egui::Id::unique(HAND_REQUEST)))
}
pub(crate) fn install_hands(ctx: &egui::Context) {
    let installed = egui::Id::unique("clipdesigner-hands-installed");
    if ctx.data(|d| d.get_temp::<bool>(installed)).is_some() {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(installed, true));
    ctx.on_end_pass(
        "clipdesigner-hands",
        std::sync::Arc::new(|ui| {
            let ctx = ui.ctx();
            let Some((grip, held)) = requested_grip(ctx) else {
                return;
            };
            ctx.data_mut(|d| d.remove::<(Grip, bool)>(egui::Id::unique(HAND_REQUEST)));
            let key = match (grip, held) {
                (Grip::Rotate, false) => "grip-rotate",
                (Grip::Rotate, true) => "grip-rotate-held",
                (Grip::Bend, false) => "grip-bend",
                (Grip::Bend, true) => "grip-bend-held",
            };
            let shown = show(key, || Some(grip_image(grip, held)));
            if shown {
                ctx.output_mut(|o| o.cursor_icon = HAND_HOST);
            }
        }),
    );
}
/// The mockup's 24-unit glyphs (option D): two arcs of a curved double arrow,
/// and a hand with the index finger pointing up.
const ROTATE: [&str; 2] = [
    "M4.6 11.2a7.6 7.6 0 0 1 13.6-3.4l1.4-1.5.6 5.6-5.5-.7 1.5-1.4a5 5 0 0 0-9 1.9z",
    "M19.4 12.8a7.6 7.6 0 0 1-13.6 3.4l-1.4 1.5-.6-5.6 5.5.7-1.5 1.4a5 5 0 0 0 9-1.9z",
];
const FINGER: &str = "M9.5 21.5c-1.8-1.6-3.6-4.2-4.5-6-.6-1.2.6-2.4 1.8-1.7l1.7 1.3V4.2c0-1.4 2.4-1.4 2.4 0V11v-1.6c0-1.3 2.3-1.3 2.3 0V11v-1c0-1.3 2.3-1.3 2.3 0v1.5-.6c0-1.3 2.2-1.3 2.2 0V16c0 2.6-1 4.3-2.5 5.5z";
/// A grip as a 32-pixel cursor, drawn from the mockup's paths at 4/3 scale.
/// Hover is white with a dark outline; held is ClipDesigner blue with a white
/// outline. The rotate arrow's hotspot is its center; the finger's is its tip.
pub(crate) fn grip_image(grip: Grip, held: bool) -> Image {
    use vello_cpu::{
        Pixmap, RenderContext,
        color::{AlphaColor, Srgb},
        kurbo::{Affine, BezPath, Cap, Join, Stroke},
    };
    const SIDE: u16 = 32;
    let paths: Vec<BezPath> = match grip {
        Grip::Rotate => ROTATE
            .iter()
            .map(|d| BezPath::from_svg(d).expect("rotate path"))
            .collect(),
        Grip::Bend => vec![BezPath::from_svg(FINGER).expect("finger path")],
    };
    let (body, edge) = if held {
        ([24, 145, 193], [255, 255, 255])
    } else {
        ([255, 255, 255], [17, 17, 17])
    };
    let mut context = RenderContext::new(SIDE, SIDE);
    context.set_transform(Affine::scale(4.0 / 3.0));
    for path in &paths {
        context.set_paint(AlphaColor::<Srgb>::from_rgba8(
            body[0], body[1], body[2], 255,
        ));
        context.fill_path(path);
        context.set_paint(AlphaColor::<Srgb>::from_rgba8(
            edge[0], edge[1], edge[2], 255,
        ));
        context.set_stroke(
            Stroke::new(1.1)
                .with_join(Join::Round)
                .with_caps(Cap::Round),
        );
        context.stroke_path(path);
    }
    let mut pixels = Pixmap::new(SIDE, SIDE);
    context.flush();
    context.render(&mut pixels, &mut vello_cpu::Resources::new());
    Image {
        size: [32, 32],
        hotspot: match grip {
            Grip::Rotate => [16, 16],
            Grip::Bend => [14, 4],
        },
        pixels: pixels
            .data()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect(),
    }
}

#[cfg(not(all(windows, not(test))))]
pub(crate) fn show(_key: &str, _build: impl FnOnce() -> Option<Image>) -> bool {
    false
}
#[cfg(not(all(windows, not(test))))]
pub(crate) fn next_frame() {}
#[cfg(not(all(windows, not(test))))]
pub(crate) fn reset() {}
#[cfg(all(windows, test))]
pub(crate) fn apply(_lparam: isize) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grips_follow_option_d_white_on_hover_and_blue_while_held() {
        let px = |i: &Image, x: usize, y: usize| {
            i.pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].to_vec()
        };
        let count = |i: &Image, f: &dyn Fn(&[u8]) -> bool| {
            (0..32 * 32)
                .filter(|n| f(&i.pixels[n * 4..n * 4 + 4]))
                .count()
        };
        for grip in [Grip::Rotate, Grip::Bend] {
            let hover = grip_image(grip, false);
            let held = grip_image(grip, true);
            let white = count(&hover, &|p| p == [255, 255, 255, 255]);
            let dark = count(&hover, &|p| p[3] > 200 && p[0] < 60);
            assert!(
                white > 40 && dark > 20,
                "{grip:?}: white body with a dark outline"
            );
            let blue = count(&held, &|p| p[3] > 200 && p[2] > 150 && p[0] < 60);
            assert!(blue > 40, "{grip:?}: held is filled blue");
            assert_eq!(px(&hover, 0, 31)[3], 0, "{grip:?}: corners stay clear");
        }
        // The rotate arrow keeps its middle clear, so the handle shows through;
        // the finger's hotspot sits on its tip.
        assert_eq!(px(&grip_image(Grip::Rotate, false), 16, 16)[3], 0);
        let finger = grip_image(Grip::Bend, false);
        assert_eq!(finger.hotspot, [14, 4]);
        assert!(
            px(&finger, 14, 6)[3] > 200,
            "the fingertip is under the hotspot"
        );
    }
    /// Writes the four grip cursors, enlarged 4x on light and dark, for review:
    /// `CLIPDESIGNER_GRIP_DIR=<dir> cargo test -p capture-desktop dump_grip_cursors -- --ignored`
    #[test]
    #[ignore = "review aid"]
    fn dump_grip_cursors() {
        let dir = std::path::PathBuf::from(std::env::var_os("CLIPDESIGNER_GRIP_DIR").unwrap());
        let mut sheet = image::RgbaImage::new(4 * 128, 2 * 128);
        for (row, bg) in [[238u8, 241, 243], [18, 24, 26]].into_iter().enumerate() {
            for (col, (grip, held)) in [
                (Grip::Rotate, false),
                (Grip::Rotate, true),
                (Grip::Bend, false),
                (Grip::Bend, true),
            ]
            .into_iter()
            .enumerate()
            {
                let g = grip_image(grip, held);
                for y in 0..128u32 {
                    for x in 0..128u32 {
                        let i = (((y / 4) * 32 + x / 4) * 4) as usize;
                        let a = u32::from(g.pixels[i + 3]);
                        let mix = |c: usize| {
                            ((u32::from(g.pixels[i + c]) * a + u32::from(bg[c]) * (255 - a)) / 255)
                                as u8
                        };
                        sheet.put_pixel(
                            col as u32 * 128 + x,
                            row as u32 * 128 + y,
                            image::Rgba([mix(0), mix(1), mix(2), 255]),
                        );
                    }
                }
            }
        }
        sheet.save(dir.join("grip-cursors.png")).unwrap();
    }
    #[test]
    fn the_cursor_under_a_hand_stays_visible() {
        // CursorIcon::None hides every cursor on Windows, the hand included.
        assert_ne!(HAND_HOST, egui::CursorIcon::None);
    }
    #[test]
    fn only_a_request_shows_a_hand() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            assert_eq!(requested_grip(ui.ctx()), None, "moving keeps its cursor");
            request_grip(ui.ctx(), Grip::Rotate, true);
            assert_eq!(requested_grip(ui.ctx()), Some((Grip::Rotate, true)));
        });
        out.textures_delta.clear();
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
