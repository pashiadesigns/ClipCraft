//! Whole-surface supersampling keeps the recorder sharp and aligned at every DPI.
//! Glyph geometry is from the approved preview; see library-icons/LICENSE.
use super::*;
use image::{RgbaImage, imageops::FilterType};
pub(super) const DIMENSIONS: [u32; 2] = [368, 114];
const PANEL: u32 = 0x0033302b;
const BLUE: u32 = 0x00c39a19;
fn rounded(x: f32, y: f32, r: [f32; 4], radius: f32) -> bool {
    let [l, t, w, h] = r;
    let cx = x.clamp(l + radius, l + w - radius);
    let cy = y.clamp(t + radius, t + h - radius);
    x >= l
        && x < l + w
        && y >= t
        && y < t + h
        && (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
}
unsafe fn box_shape(dc: HDC, r: [i32; 4], radius: i32, fill: u32, edge: u32, width: i32) {
    unsafe {
        let brush = Bitmap(CreateSolidBrush(fill));
        let pen = Bitmap(CreatePen(PS_SOLID, width, edge));
        let b = SelectObject(dc, brush.0);
        let p = SelectObject(dc, pen.0);
        RoundRect(
            dc,
            r[0],
            r[1],
            r[0] + r[2],
            r[1] + r[3],
            radius * 2,
            radius * 2,
        );
        SelectObject(dc, b);
        SelectObject(dc, p);
    }
}
unsafe fn text(dc: HDC, value: &str, size: i32, color: u32, mut r: RECT, flags: u32) {
    unsafe {
        let font = Bitmap(CreateFontW(
            -size,
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
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32,
            w!("Segoe UI"),
        ));
        let old = SelectObject(dc, font.0);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        let mut value: Vec<u16> = value.encode_utf16().collect();
        DrawTextW(dc, value.as_mut_ptr(), value.len() as i32, &mut r, flags);
        SelectObject(dc, old);
    }
}
unsafe fn glyph(dc: HDC, name: &str, x: i32, y: i32, color: u32) {
    // Draw the original curves at the recorder's 4x composition resolution.
    // AlphaBlend preserves antialiasing and separate subpaths without miter spikes.
    static GLYPHS: OnceLock<std::sync::Mutex<std::collections::HashMap<String, RgbaImage>>> =
        OnceLock::new();
    let mut glyphs = GLYPHS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let image = glyphs
        .entry(name.to_owned())
        .or_insert_with(|| crate::ui_glyphs::render(name, 80).expect("bundled recorder glyph"));
    unsafe {
        let saved = SaveDC(dc);
        SetMapMode(dc, MM_TEXT);
        let source = MemoryDc(CreateCompatibleDC(dc));
        let mut bits = null_mut();
        let bitmap = Bitmap(CreateDIBSection(
            source.0,
            &bitmap_info([80, 80]),
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        ));
        if !bitmap.0.is_null() {
            let dst = std::slice::from_raw_parts_mut(bits.cast::<u8>(), 80 * 80 * 4);
            let rgb = [
                (color & 255) as u8,
                ((color >> 8) & 255) as u8,
                ((color >> 16) & 255) as u8,
            ];
            for (pixel, out) in image.pixels().zip(dst.as_chunks_mut::<4>().0) {
                let a = pixel[3] as u16;
                out.copy_from_slice(&[
                    (rgb[2] as u16 * a / 255) as u8,
                    (rgb[1] as u16 * a / 255) as u8,
                    (rgb[0] as u16 * a / 255) as u8,
                    pixel[3],
                ]);
            }
            let old = SelectObject(source.0, bitmap.0);
            AlphaBlend(
                dc,
                x * 4,
                y * 4,
                80,
                80,
                source.0,
                0,
                0,
                80,
                80,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            );
            SelectObject(source.0, old);
        }
        RestoreDC(dc, saved);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::capture::windows) enum Caption {
    Steps,
    Scrolling {
        automatic: bool,
        running: bool,
        countdown: u32,
    },
    Video {
        seconds: u64,
        countdown: u32,
        /// Step by Step recordings count their steps (clicks), like image Steps.
        steps: Option<usize>,
        /// The webcam is recording too.
        webcam: bool,
    },
}
pub(super) fn render(
    dpi: i32,
    expanded: f32,
    count: usize,
    paused: bool,
    hover: u32,
    pulse: f32,
) -> Result<RgbaImage, String> {
    render_capture(dpi, expanded, count, paused, hover, pulse, Caption::Steps)
}
pub(in crate::capture::windows) fn render_capture(
    dpi: i32,
    expanded: f32,
    count: usize,
    paused: bool,
    hover: u32,
    pulse: f32,
    caption: Caption,
) -> Result<RgbaImage, String> {
    let size = [DIMENSIONS[0] * 4, DIMENSIONS[1] * 4];
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
        if bitmap.0.is_null() || dc.0.is_null() {
            return Err(win_error("Cannot draw recorder"));
        }
        let old = SelectObject(dc.0, bitmap.0);
        SetMapMode(dc.0, MM_ANISOTROPIC);
        SetWindowExtEx(dc.0, 1, 1, null_mut());
        SetViewportExtEx(dc.0, 4, 4, null_mut());
        let video = matches!(caption, Caption::Video { .. });
        let width = 112. + (if video { 152. } else { 256. }) * expanded;
        let left = (368. - width) / 2.;
        box_shape(
            dc.0,
            [left.round() as i32, 1, width.round() as i32 - 1, 112],
            20,
            PANEL,
            if expanded > 0. { BLUE } else { PANEL },
            2,
        );
        for (id, x, label, name) in [
            (5, 23, "Undo", "undo-2"),
            (
                3,
                81,
                if matches!(caption, Caption::Scrolling { .. }) {
                    "Reset"
                } else {
                    "Restart"
                },
                "rotate-ccw",
            ),
            (4, 235, "Cancel", "x"),
            (2, 293, "Finish", "check"),
        ] {
            if expanded <= 0. || matches!(caption, Caption::Video { .. }) && [3, 5].contains(&id) {
                continue;
            }
            let x = if video {
                if id == 4 { 69 } else { 247 }
            } else {
                x
            };
            let x = x + (if x < 184 { 12. } else { -12. } * (1. - expanded)).round() as i32;
            let color = if id == 4 { 0x00a49cff } else { 0x00f8f7f5 };
            if id == 2 || hover == id {
                box_shape(
                    dc.0,
                    [x + 7, 23, 38, 36],
                    9,
                    if id == 2 { BLUE } else { 0x0046423b },
                    if id == 2 { BLUE } else { 0x0046423b },
                    1,
                );
            }
            glyph(
                dc.0,
                name,
                x + 16,
                31,
                if id == 5 && count == 0 {
                    0x0067615c
                } else {
                    color
                },
            );
            text(
                dc.0,
                label,
                11,
                if id == 5 && count == 0 {
                    0x0067615c
                } else {
                    0x00f8f7f5
                },
                RECT {
                    left: x,
                    top: 65,
                    right: x + 52,
                    bottom: 83,
                },
                DT_CENTER | DT_SINGLELINE | DT_VCENTER,
            );
        }
        box_shape(
            dc.0,
            [156, 14, 56, 56],
            28,
            PANEL,
            if paused { 0x004eb4ed } else { 0x00665556 },
            1,
        );
        box_shape(
            dc.0,
            [161, 19, 46, 46],
            23,
            if paused { 0x0028343c } else { 0x005445ed },
            if paused { 0x0028343c } else { 0x00645afa },
            1,
        );
        if let Caption::Scrolling {
            countdown,
            running,
            automatic,
        } = caption
        {
            if countdown > 0 {
                text(
                    dc.0,
                    &countdown.to_string(),
                    24,
                    0x00ffffff,
                    RECT {
                        left: 156,
                        top: 14,
                        right: 212,
                        bottom: 70,
                    },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            } else if running && automatic && hover == 1 {
                box_shape(dc.0, [176, 34, 16, 16], 2, 0x00ffffff, 0x00ffffff, 1);
            }
        } else if let Caption::Video { countdown, .. } = caption
            && countdown > 0
        {
            text(
                dc.0,
                &countdown.to_string(),
                24,
                0x00ffffff,
                RECT {
                    left: 156,
                    top: 14,
                    right: 212,
                    bottom: 70,
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        } else if paused {
            glyph(dc.0, "play", 174, 32, 0x004eb4ed);
        } else if hover == 1 {
            glyph(dc.0, "pause", 174, 32, 0x00ffffff);
        }
        if let Caption::Video { webcam: true, .. } = caption {
            // The webcam indicator sits beside the record button (BG mockup note),
            // on its own disc so it reads on the collapsed controls too.
            if expanded <= 0. {
                box_shape(dc.0, [211, 15, 30, 30], 15, PANEL, 0x00665556, 1);
            }
            glyph(dc.0, "video", 216, 20, 0x00edb47d);
        }
        box_shape(dc.0, [135, 78, 98, 29], 8, PANEL, PANEL, 1);
        if caption == Caption::Steps {
            let number = count.to_string();
            let chars = number.len() as i32;
            let start = 184 - (chars * 10 + 5 + 32) / 2;
            text(
                dc.0,
                &number,
                18,
                0x00f8f7f5,
                RECT {
                    left: start,
                    top: 79,
                    right: start + chars * 11,
                    bottom: 104,
                },
                DT_LEFT | DT_SINGLELINE | DT_VCENTER,
            );
            text(
                dc.0,
                if count == 1 { "step" } else { "steps" },
                13,
                0x00c8c3ba,
                RECT {
                    left: start + chars * 10 + 5,
                    top: 83,
                    right: 234,
                    bottom: 103,
                },
                DT_LEFT | DT_SINGLELINE | DT_VCENTER,
            );
        } else {
            let label = match caption {
                Caption::Scrolling { automatic, .. } => {
                    if automatic {
                        "Automatic".into()
                    } else {
                        "Manual".into()
                    }
                }
                Caption::Video { countdown, .. } if countdown > 0 => "Starting…".into(),
                Caption::Video {
                    seconds,
                    steps: Some(n),
                    ..
                } => format!(
                    "{n} {} · {:02}:{:02}",
                    if n == 1 { "step" } else { "steps" },
                    seconds / 60,
                    seconds % 60
                ),
                Caption::Video { seconds, .. } => {
                    format!("{:02}:{:02}", seconds / 60, seconds % 60)
                }
                Caption::Steps => unreachable!(),
            };
            let small = matches!(caption, Caption::Video { steps: Some(_), .. });
            text(
                dc.0,
                &label,
                if small { 11 } else { 13 },
                0x00f8f7f5,
                RECT {
                    left: 135,
                    top: 78,
                    right: 233,
                    bottom: 107,
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        GdiFlush();
        SelectObject(dc.0, old);
        let caption_kind = caption;
        let bytes = std::slice::from_raw_parts(bits.cast::<u8>(), (size[0] * size[1] * 4) as usize);
        let mut rgba = RgbaImage::new(size[0], size[1]);
        for (x, y, p) in rgba.enumerate_pixels_mut() {
            let sx = (x as f32 + 0.5) / 4.;
            let sy = (y as f32 + 0.5) / 4.;
            let i = ((y * size[0] + x) * 4) as usize;
            let shell = rounded(sx, sy, [left, 0., width, 114.], 20.);
            let center = (sx - 184.).powi(2) + (sy - 42.).powi(2) <= 28. * 28.;
            let caption = rounded(sx, sy, [135., 78., 98., 29.], 8.);
            let camera = matches!(caption_kind, Caption::Video { webcam: true, .. })
                && (sx - 226.).powi(2) + (sy - 30.).powi(2) <= 15. * 15.;
            let alpha = if center || caption || camera {
                255
            } else if shell {
                (255. * expanded).round() as u8
            } else {
                0
            };
            *p = image::Rgba([bytes[i + 2], bytes[i + 1], bytes[i], alpha]);
            if pulse > 0. && !paused {
                let radius = 28. + 6. * pulse;
                let distance = ((sx - 184.).powi(2) + (sy - 42.).powi(2)).sqrt();
                if (distance - radius).abs() < 0.65 {
                    let a = (1. - pulse) * 0.6;
                    for (v, c) in p.0[..3].iter_mut().zip([237., 69., 84.]) {
                        *v = (*v as f32 * (1. - a) + c * a) as u8;
                    }
                    p[3] = p[3].max((a * 255.) as u8);
                }
            }
        }
        if video {
            rgba =
                image::imageops::crop_imm(&rgba, 52 * 4, 0, 264 * 4, DIMENSIONS[1] * 4).to_image();
        }
        let target = [
            ((if video { 264 } else { DIMENSIONS[0] }) * dpi as u32 + 48) / 96,
            (DIMENSIONS[1] * dpi as u32 + 48) / 96,
        ];
        // Premultiplied filtering prevents fringes at transparent edges.
        for p in rgba.pixels_mut() {
            for c in 0..3 {
                p[c] = ((p[c] as u16 * p[3] as u16 + 127) / 255) as u8;
            }
        }
        Ok(image::imageops::resize(
            &rgba,
            target[0],
            target[1],
            FilterType::Lanczos3,
        ))
    }
}
type SurfaceKey = (i32, u16, usize, bool, u32);
#[derive(Default)]
struct SurfaceCache {
    base: Option<(SurfaceKey, RgbaImage)>,
}
impl SurfaceCache {
    fn frame(&mut self, key: SurfaceKey, pulse: f32) -> Result<RgbaImage, String> {
        if self.base.as_ref().is_none_or(|(old, _)| *old != key) {
            self.base = Some((
                key,
                render(key.0, key.1 as f32 / 256., key.2, key.3, key.4, 0.)?,
            ));
        }
        let mut image = self.base.as_ref().unwrap().1.clone();
        // Only the small ring changes during steady recording. Composite it at
        // final display resolution; never rerasterize text/icons for the pulse.
        if !key.3 {
            pulse_ring(&mut image, key.0, pulse);
        }
        Ok(image)
    }
}
pub(in crate::capture::windows) fn pulse_ring(image: &mut RgbaImage, dpi: i32, pulse: f32) {
    if pulse > 0. && pulse <= 1. {
        let scale = dpi as f32 / 96.;
        let radius = (28. + 6. * pulse) * scale;
        let cx = image.width() as f32 / 2.;
        let cy = 42. * scale;
        let left = (cx - radius - 2.).max(0.) as u32;
        let top = (cy - radius - 2.).max(0.) as u32;
        let right = (cx + radius + 2.).ceil().min(image.width() as f32) as u32;
        let bottom = (cy + radius + 2.).ceil().min(image.height() as f32) as u32;
        for y in top..bottom {
            for x in left..right {
                let distance =
                    ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let coverage = (0.65 * scale + 0.5 - (distance - radius).abs()).clamp(0., 1.);
                let alpha = coverage * (1. - pulse) * 0.6;
                if alpha > 0. {
                    let p = image.get_pixel_mut(x, y);
                    for (v, c) in p.0[..3].iter_mut().zip([237., 69., 84.]) {
                        *v = (*v as f32 * (1. - alpha) + c * alpha).round() as u8;
                    }
                    p[3] = (p[3] as f32 * (1. - alpha) + 255. * alpha).round() as u8;
                }
            }
        }
    }
}
thread_local! {static SURFACE:RefCell<SurfaceCache>=RefCell::new(SurfaceCache::default());}
pub(super) fn present(hwnd: HWND, state: &Dialog, expanded: f32) {
    let pulse = if state.animate && !state.paused && !state.finish {
        state.began.elapsed().as_secs_f32() % 1.5 / 0.75
    } else {
        0.
    };
    let key = (
        state.dpi,
        (expanded.clamp(0., 1.) * 256.).round() as u16,
        state.count,
        state.paused,
        state.hover,
    );
    let Ok(image) = SURFACE.with(|s| s.borrow_mut().frame(key, pulse)) else {
        return;
    };
    present_image(hwnd, &image);
}
pub(in crate::capture::windows) fn present_image(hwnd: HWND, image: &RgbaImage) {
    if !super::super::control_surface::layered(hwnd) {
        super::super::control_surface::present(hwnd, image);
        return;
    }
    unsafe {
        let dc = MemoryDc(CreateCompatibleDC(null_mut()));
        let mut bits = null_mut();
        let bitmap = Bitmap(CreateDIBSection(
            dc.0,
            &bitmap_info([image.width(), image.height()]),
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        ));
        if bitmap.0.is_null() {
            return;
        }
        let dst = std::slice::from_raw_parts_mut(bits.cast::<u8>(), image.as_raw().len());
        for (src, dst) in image
            .as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(dst.as_chunks_mut::<4>().0)
        {
            dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
        }
        let old = SelectObject(dc.0, bitmap.0);
        let size = SIZE {
            cx: image.width() as i32,
            cy: image.height() as i32,
        };
        let from = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        UpdateLayeredWindow(
            hwnd,
            null_mut(),
            null(),
            &size,
            dc.0,
            &from,
            0,
            &blend,
            ULW_ALPHA,
        );
        SelectObject(dc.0, old);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scroll_and_video_controller_surfaces_are_crisp_at_each_dpi() {
        for dpi in [96, 144, 192] {
            for (name, caption, paused) in [
                (
                    "scroll-ready",
                    Caption::Scrolling {
                        automatic: true,
                        running: false,
                        countdown: 0,
                    },
                    false,
                ),
                (
                    "scroll-countdown",
                    Caption::Scrolling {
                        automatic: true,
                        running: false,
                        countdown: 3,
                    },
                    false,
                ),
                (
                    "scroll-manual",
                    Caption::Scrolling {
                        automatic: false,
                        running: true,
                        countdown: 0,
                    },
                    false,
                ),
                (
                    "video-recording",
                    Caption::Video {
                        seconds: 65,
                        countdown: 0,
                        steps: None,
                        webcam: false,
                    },
                    false,
                ),
                (
                    "video-countdown",
                    Caption::Video {
                        seconds: 0,
                        countdown: 3,
                        steps: None,
                        webcam: false,
                    },
                    false,
                ),
                (
                    "video-paused",
                    Caption::Video {
                        seconds: 65,
                        countdown: 0,
                        steps: None,
                        webcam: false,
                    },
                    true,
                ),
                (
                    "video-steps-webcam",
                    Caption::Video {
                        seconds: 754,
                        countdown: 0,
                        steps: Some(12),
                        webcam: true,
                    },
                    false,
                ),
            ] {
                for expanded in [0., 1.] {
                    let pixels = render_capture(dpi, expanded, 1, paused, 1, 0., caption).unwrap();
                    assert_eq!(
                        pixels.dimensions(),
                        (
                            ((if matches!(caption, Caption::Video { .. }) {
                                264
                            } else {
                                368
                            }) * dpi as u32
                                + 48)
                                / 96,
                            (114 * dpi as u32 + 48) / 96
                        )
                    );
                    assert_eq!(pixels.get_pixel(0, 0)[3], 0);
                    if let Some(root) = std::env::var_os("CLIPCRAFT_VIDEO_EVIDENCE_DIR") {
                        pixels
                            .save(
                                std::path::PathBuf::from(root)
                                    .join(format!("{name}-{dpi}-{expanded}.png")),
                            )
                            .unwrap();
                    }
                }
            }
        }
    }
    #[test]
    fn steady_pulse_reuses_vector_surface_and_has_bounded_frame_cost() {
        let mut reports = Vec::new();
        for dpi in [96, 192, 384] {
            let mut cache = SurfaceCache::default();
            let key = (dpi, 256, 12, false, 0);
            cache.frame(key, 0.).unwrap();
            let pointer = cache.base.as_ref().unwrap().1.as_ptr();
            let mut elapsed = Vec::new();
            for n in 1..=30 {
                let started = std::time::Instant::now();
                let image = cache.frame(key, n as f32 / 30.).unwrap();
                std::hint::black_box(image);
                elapsed.push(started.elapsed().as_secs_f64() * 1000.);
                assert_eq!(cache.base.as_ref().unwrap().1.as_ptr(), pointer);
            }
            elapsed.sort_by(f64::total_cmp);
            reports.push(serde_json::json!({"dpi":dpi,"median_ms":elapsed[15],"p95_ms":elapsed[28],"static_surface_reused":true}));
        }
        if let Ok(root) = std::env::var("CLIPCRAFT_CURSOR_EVIDENCE_DIR") {
            std::fs::write(
                std::path::Path::new(&root).join("recorder-frame-profile.json"),
                serde_json::to_vec_pretty(&reports).unwrap(),
            )
            .unwrap();
        }
    }
    #[test]
    fn recorder_states_render_offscreen_at_multiple_dpi() {
        for dpi in [96, 120, 144, 192, 384] {
            let mut states = Vec::new();
            for (name, expanded, paused, hover) in [
                ("compact", 0., false, 0),
                ("recording", 1., false, 0),
                ("pause-hover", 1., false, 1),
                ("paused", 1., true, 1),
            ] {
                let image = render(dpi, expanded, 12, paused, hover, 0.).unwrap();
                assert_eq!(
                    image.dimensions(),
                    ((368 * dpi as u32 + 48) / 96, (114 * dpi as u32 + 48) / 96)
                );
                assert_eq!(image.get_pixel(0, 0)[3], 0);
                if let Some(dir) = std::env::var_os("CLIPCRAFT_CURSOR_EVIDENCE_DIR") {
                    image
                        .save(
                            std::path::PathBuf::from(dir)
                                .join(format!("recorder-{name}-{dpi}dpi.png")),
                        )
                        .unwrap();
                }
                states.push(image);
            }
            assert_ne!(states[0], states[1]);
            assert_ne!(states[1], states[2]);
            assert_ne!(states[2], states[3]);
        }
    }
}
