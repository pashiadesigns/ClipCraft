//! A bounded, excluded floating preview. The stitched image is shared; only the
//! visible preview crop is resampled when content, size or scroll offset changes.
use super::*;
use std::sync::Arc;
mod surface;
pub(super) struct Preview {
    state: Box<State>,
}
struct State {
    hwnd: HWND,
    work: RECT,
    dpi: f32,
    image: Option<Arc<image::RgbaImage>>,
    expanded: bool,
    compact_position: Option<[i32; 2]>,
    expand_requested: bool,
    offset: u32,
    pixels: Vec<u8>,
    size: [u32; 2],
    status: String,
    dirty: bool,
    paint_dirty: bool,
    surface: Option<image::RgbaImage>,
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                (*(l as *const CREATESTRUCTW)).lpCreateParams as isize,
            );
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if !ptr.is_null() {
            let s = &mut *ptr;
            match msg {
                WM_CLOSE => return 0,
                WM_EXITSIZEMOVE => {
                    let mut monitor = MONITORINFO {
                        cbSize: size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    if GetMonitorInfoW(
                        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                        &mut monitor,
                    ) != 0
                    {
                        s.work = monitor.rcWork;
                    }
                    s.dirty = true;
                    return 0;
                }
                WM_ERASEBKGND => return 1,
                WM_DPICHANGED => {
                    s.dpi = ((w >> 16) as u16) as f32 / 96.;
                    s.dirty = true;
                    return 0;
                }
                WM_NCHITTEST => {
                    let mut p = POINT {
                        x: l as u16 as i16 as i32,
                        y: (l >> 16) as u16 as i16 as i32,
                    };
                    ScreenToClient(hwnd, &mut p);
                    if p.y < (34. * s.dpi) as i32 {
                        return HTCAPTION as LRESULT;
                    }
                }
                WM_LBUTTONUP => {
                    let mut r = RECT::default();
                    GetClientRect(hwnd, &mut r);
                    let x = l as u16 as i16 as i32;
                    let y = (l >> 16) as u16 as i16 as i32;
                    let button = expand_rect([r.right as u32, r.bottom as u32], s);
                    if s.image.is_some()
                        && x >= button.left
                        && x < button.right
                        && y >= button.top
                        && y < button.bottom
                    {
                        if !s.expanded {
                            let mut r = RECT::default();
                            GetWindowRect(hwnd, &mut r);
                            s.compact_position = Some([r.left, r.top]);
                        }
                        s.expanded = !s.expanded;
                        s.expand_requested = true;
                        s.dirty = true;
                    }
                    return 0;
                }
                WM_MOUSEWHEEL => {
                    if let Some(image) = &s.image {
                        let delta = (w >> 16) as u16 as i16 as i32;
                        let step = (image.height() / 20).max(32);
                        s.offset = if delta > 0 {
                            s.offset.saturating_sub(step)
                        } else {
                            s.offset.saturating_add(step).min(image.height() - 1)
                        };
                        s.dirty = true;
                    }
                    return 0;
                }
                WM_KEYDOWN if w == VK_ESCAPE as usize && s.expanded => {
                    s.expanded = false;
                    s.dirty = true;
                    return 0;
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    BeginPaint(hwnd, &mut ps);
                    if let Some(image) = &s.surface {
                        super::step_recorder::visual::present_image(hwnd, image);
                    }
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                _ => (),
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.state.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.state.hwnd);
        }
    }
}
impl Preview {
    pub fn new(owner: HWND, work: RECT, dpi: i32) -> Result<Self, String> {
        unsafe {
            static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
            CLASS
                .get_or_init(|| {
                    let c = WNDCLASSW {
                        hIcon: crate::brand::native_icon(),
                        lpfnWndProc: Some(procedure),
                        hInstance: GetModuleHandleW(null()),
                        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                        lpszClassName: w!("ClipCraftScrollPreview"),
                        ..Default::default()
                    };
                    let a = RegisterClassW(&c);
                    if a == 0 {
                        Err(win_error("Cannot register Preview"))
                    } else {
                        Ok(a)
                    }
                })
                .as_ref()
                .map_err(Clone::clone)?;
            let mut state = Box::new(State {
                hwnd: null_mut(),
                work,
                dpi: dpi as f32 / 96.,
                image: None,
                expanded: false,
                compact_position: None,
                expand_requested: false,
                offset: 0,
                pixels: Vec::new(),
                size: [0, 0],
                status: "Click Record to begin.".into(),
                dirty: true,
                paint_dirty: true,
                surface: None,
            });
            let width = (222. * state.dpi) as i32;
            let inset = (12. * state.dpi) as i32;
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
                w!("ClipCraftScrollPreview"),
                w!("Preview · drag this header to move"),
                WS_POPUP,
                work.right - width - inset,
                work.top + inset,
                width,
                180,
                owner,
                null_mut(),
                GetModuleHandleW(null()),
                (&mut *state as *mut State).cast(),
            );
            if hwnd.is_null() {
                return Err(win_error("Cannot create Preview"));
            }
            state.hwnd = hwnd;
            let mut result = Self { state };
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
                return Err(win_error("Cannot exclude Preview from screenshots"));
            }
            result.refresh()?;
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(result)
        }
    }
    pub fn set_image(&mut self, image: Option<Arc<image::RgbaImage>>) {
        let same = match (&image, &self.state.image) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if !same {
            self.state.image = image;
            self.state.dirty = true;
        }
    }
    pub fn status(&mut self, text: &str) {
        if self.state.status != text {
            self.state.status = text.into();
            self.state.paint_dirty = true;
        }
    }
    pub fn expanded_requested(&mut self) -> bool {
        std::mem::take(&mut self.state.expand_requested)
    }
    pub fn refresh(&mut self) -> Result<(), String> {
        let s = &mut *self.state;
        if !s.dirty && !s.paint_dirty {
            return Ok(());
        }
        if !s.dirty {
            let mut r = RECT::default();
            unsafe {
                GetClientRect(s.hwnd, &mut r);
            }
            s.surface = Some(surface::render([r.right as u32, r.bottom as u32], s)?);
            super::step_recorder::visual::present_image(s.hwnd, s.surface.as_ref().unwrap());
            s.paint_dirty = false;
            return Ok(());
        }
        s.dirty = false;
        s.paint_dirty = false;
        let maxw = (s.work.right - s.work.left - 24).max(100) as u32;
        let maxh = (s.work.bottom - s.work.top - 24).max(100) as u32;
        let frame = (4. * s.dpi) as u32;
        let width = if s.expanded {
            s.image
                .as_ref()
                .map_or((222. * s.dpi) as u32, |image| image.width() + frame)
                .clamp((180. * s.dpi) as u32, (900. * s.dpi) as u32)
        } else {
            (222. * s.dpi) as u32
        }
        .min(maxw);
        let header = (36. * s.dpi) as u32;
        let footer = (if s.expanded { 40. } else { 72. } * s.dpi) as u32;
        let mut body = (88. * s.dpi) as u32;
        s.pixels.clear();
        s.size = [0, 0];
        if let Some(image) = &s.image {
            let scale = ((width - frame) as f32 / image.width() as f32).min(1.);
            let cap = if s.expanded {
                (650. * s.dpi) as u32
            } else {
                (330. * s.dpi) as u32
            }
            .min(maxh.saturating_sub(header + footer + frame))
            .max(1);
            body = ((image.height() as f32 * scale).ceil() as u32).clamp(1, cap);
            let rows = ((body as f32 / scale).ceil() as u32).min(image.height());
            s.offset = s.offset.min(image.height() - rows);
            let crop = image::imageops::crop_imm(image.as_ref(), 0, s.offset, image.width(), rows)
                .to_image();
            s.size = [(image.width() as f32 * scale).round().max(1.) as u32, body];
            let pixels = image::imageops::resize(
                &crop,
                s.size[0],
                body,
                image::imageops::FilterType::Triangle,
            );
            s.pixels = pixels.into_raw();
            for p in s.pixels.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
        }
        let height = (header + footer + body + frame).min(maxh);
        unsafe {
            let mut r = RECT::default();
            GetWindowRect(s.hwnd, &mut r);
            let position = if !s.expanded {
                s.compact_position.take().unwrap_or([r.left, r.top])
            } else {
                [r.left, r.top]
            };
            let [x, y] = contained_position(s.work, position, [width, height]);
            SetWindowPos(
                s.hwnd,
                HWND_TOPMOST,
                x,
                y,
                width as i32,
                height as i32,
                SWP_NOACTIVATE,
            );
            s.surface = Some(surface::render([width, height], s)?);
            super::step_recorder::visual::present_image(s.hwnd, s.surface.as_ref().unwrap());
        }
        Ok(())
    }
}
fn contained_position(work: RECT, position: [i32; 2], size: [u32; 2]) -> [i32; 2] {
    [
        position[0].clamp(work.left, (work.right - size[0] as i32).max(work.left)),
        position[1].clamp(work.top, (work.bottom - size[1] as i32).max(work.top)),
    ]
}
fn expand_rect(size: [u32; 2], s: &State) -> RECT {
    let px = |n: f32| (n * s.dpi).round() as i32;
    let top = if s.size[1] < px(44.) as u32 {
        px(3.)
    } else {
        px(36.) + s.size[1] as i32 - px(38.)
    };
    RECT {
        left: size[0] as i32 - px(38.),
        top,
        right: size[0] as i32 - px(8.),
        bottom: top + px(30.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_scroll_preview_renders_waiting_live_and_expanded() {
        let mut document = capture_core::Document::blank(720, 2160).unwrap();
        for (i, title) in [
            "Prepare your workspace",
            "Choose the capture area",
            "Capture the entire process",
            "Review every detail",
            "Add clear annotations",
            "Share your finished guide",
        ]
        .iter()
        .enumerate()
        {
            let y = 36. + i as f32 * 350.;
            let mut title_object = capture_core::steps::marker(
                i as u64 * 2 + 1,
                [60., y + 40.],
                40,
                [24, 145, 193, 255],
                (i as u32 + 1, false),
                [720, 2160],
            )
            .unwrap();
            title_object.kind = capture_core::Kind::Text;
            title_object.start = [35., y];
            title_object.end = [690., y + 70.];
            let text = title_object.text.as_mut().unwrap();
            text.content = format!("{}. {title}", i + 1);
            text.size_points = 24.;
            let mut body = title_object.clone();
            body.id += 1;
            body.start = [35., y + 80.];
            body.end = [680., y + 240.];
            body.color = [65, 79, 87, 255];
            let text = body.text.as_mut().unwrap();
            text.size_points = 18.;
            text.content="Keep the important information in view.\nScroll steadily, then inspect the preview.\nUndo a scroll burst or reset to try again.".into();
            document.annotations.extend([title_object, body]);
        }
        let source = Arc::new(document.render().unwrap());
        for dpi in [96, 144, 192] {
            for (name, expanded, has_image) in [
                ("waiting", false, false),
                ("live", false, true),
                ("expanded", true, true),
            ] {
                unsafe {
                    let hwnd = CreateWindowExW(
                        WS_EX_LAYERED,
                        w!("STATIC"),
                        w!("Offscreen Preview fixture"),
                        WS_POPUP,
                        0,
                        0,
                        222,
                        180,
                        null_mut(),
                        null_mut(),
                        GetModuleHandleW(null()),
                        null(),
                    );
                    assert!(!hwnd.is_null());
                    let mut preview=Preview{state:Box::new(State{compact_position:None,hwnd,work:RECT{left:0,top:0,right:2560,bottom:1440},dpi:dpi as f32/96.,image:has_image.then(||source.clone()),expanded,expand_requested:false,offset:0,pixels:vec![],size:[0,0],status:if expanded{"Inspection · scroll to review · click the corner to return"}else if has_image{"720 × 2160 px · Manual\nScroll to capture. Undo removes the last burst."}else{"Click Record to begin."}.into(),dirty:true,paint_dirty:true,surface:None})};
                    preview.refresh().unwrap();
                    assert_eq!(IsWindowVisible(hwnd), 0);
                    let mut r = RECT::default();
                    GetClientRect(hwnd, &mut r);
                    let size = [r.right as u32, r.bottom as u32];
                    assert!(size[0] <= 2560 && size[1] <= 1440);
                    let pixels = preview.state.surface.as_ref().unwrap().clone();
                    assert_eq!(pixels.get_pixel(0, 0)[3], 0);
                    assert!(pixels.pixels().any(|p| p[3] > 0 && p[3] < 255));
                    if expanded {
                        assert_eq!(size[0], source.width() + (4. * preview.state.dpi) as u32);
                    }
                    if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EVIDENCE_DIR") {
                        let folder = std::path::PathBuf::from(folder);
                        std::fs::create_dir_all(&folder).unwrap();
                        pixels
                            .save(folder.join(format!("scroll-preview-{name}-{dpi}.png")))
                            .unwrap();
                        if dpi == 96 && name == "live" {
                            let mut process = image::RgbaImage::from_pixel(
                                1280,
                                800,
                                image::Rgba([22, 27, 30, 255]),
                            );
                            let page = image::imageops::crop_imm(source.as_ref(), 0, 0, 720, 680)
                                .to_image();
                            image::imageops::overlay(&mut process, &page, 100, 120);
                            image::imageops::overlay(&mut process, &pixels, 1046, 12);
                            let recorder = super::super::step_recorder::visual::render_capture(
                                96,
                                1.,
                                1,
                                false,
                                0,
                                0.,
                                super::super::step_recorder::visual::Caption::Scrolling {
                                    automatic: false,
                                    running: false,
                                    countdown: 0,
                                },
                            )
                            .unwrap();
                            image::imageops::overlay(&mut process, &recorder, 456, 6);
                            process.save(folder.join("scroll-process.png")).unwrap();
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn preview_growth_preserves_dragged_position_and_stays_inside_work_area() {
        for work in [
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            RECT {
                left: -1920,
                top: 100,
                right: 0,
                bottom: 1140,
            },
        ] {
            for scale in [1., 1.5, 2.] {
                let width = (222. * scale) as u32;
                let inset = (12. * scale) as i32;
                let top_right = [work.right - width as i32 - inset, work.top + inset];
                assert_eq!(contained_position(work, top_right, [width, 450]), top_right);
                let dragged = [work.left + 70, work.top + 120];
                assert_eq!(contained_position(work, dragged, [width, 600]), dragged);
                let clipped =
                    contained_position(work, [work.right - 30, work.bottom - 40], [width, 600]);
                assert_eq!(clipped, [work.right - width as i32, work.bottom - 600]);
            }
        }
    }
}
