//! All HWND/GDI resources stay on this worker thread. No screenshot is written
//! to disk: only the selected pixels enter the normal editable document engine.
use super::{CaptureResult, Region, crop_bgra, repaint_selection};
use capture_core::Document;
use std::{
    mem::size_of,
    ptr::{null, null_mut},
    sync::OnceLock,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{
        Dwm::{
            DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_TRANSITIONS_FORCEDISABLED, DwmFlush,
            DwmGetWindowAttribute, DwmSetWindowAttribute,
        },
        Gdi::*,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
use windows_sys::core::{BOOL, w};
mod countdown;
mod cursor;
mod monitor_picker;
mod region_preview;
mod restart;
mod scrolling;
mod scrolling_preview;
mod step_context;
mod step_recorder;
mod steps_intro;
mod transport;
pub(crate) mod video_audio;
pub(crate) mod video_encoder;
mod video_recording;

fn win_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

struct ScreenDc(HDC);
impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(null_mut(), self.0);
        }
    }
}
struct MemoryDc(HDC);
impl Drop for MemoryDc {
    fn drop(&mut self) {
        unsafe {
            DeleteDC(self.0);
        }
    }
}
struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}
struct RestoreEditor(HWND, bool);
fn hide_for_capture(owner: HWND) -> Result<(), String> {
    // Per-window setting only: suppress the OS show/hide transition before
    // hiding this editor. Keep it disabled when restoring the capture preview.
    let disabled: BOOL = 1;
    unsafe {
        let status = DwmSetWindowAttribute(
            owner,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            (&disabled as *const BOOL).cast(),
            size_of::<BOOL>() as u32,
        );
        if status < 0 {
            return Err(format!(
                "Could not hide ClipCraft without animation ({status:#x}). Try the capture again."
            ));
        }
        ShowWindow(owner, SW_HIDE);
        if IsWindowVisible(owner) != 0 {
            return Err(
                "ClipCraft is still visible. Close any open dialog and try the capture again."
                    .into(),
            );
        }
        if DwmFlush() < 0 {
            return Err("Windows could not prepare the screen for capture. Try again.".into());
        }
    }
    Ok(())
}
impl Drop for RestoreEditor {
    fn drop(&mut self) {
        unsafe {
            // Restore only the pre-capture visibility. The editor's background
            // logic decides whether a successful preview should open/focus it.
            if self.1 {
                // SW_SHOWNOACTIVATE restores normal size; SW_SHOWNA preserves
                // the existing maximized/normal placement while showing it.
                ShowWindow(self.0, SW_SHOWNA);
            }
        }
    }
}
struct DpiContext(DPI_AWARENESS_CONTEXT);
impl Drop for DpiContext {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}

struct Snapshot {
    origin: [i32; 2],
    size: [u32; 2],
    pixels: Vec<u8>,
    dimmed: Vec<u8>,
    cursor: Option<capture_core::captured_cursor::Cursor>,
}

fn bitmap_info(size: [u32; 2]) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size[0] as i32,
            biHeight: -(size[1] as i32), // top-down, matching pointer coordinates
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    }
}

impl Snapshot {
    fn capture_window(hwnd: HWND, include_cursor: bool) -> Result<Self, String> {
        unsafe {
            let mut bounds = RECT::default();
            if GetWindowRect(hwnd, &mut bounds) == 0 {
                return Err(win_error("The selected window is no longer available"));
            }
            let size = [
                (bounds.right - bounds.left).max(1) as u32,
                (bounds.bottom - bounds.top).max(1) as u32,
            ];
            capture_core::validate_dimensions(size[0], size[1]).map_err(|e| e.to_string())?;
            let memory = MemoryDc(CreateCompatibleDC(null_mut()));
            let mut bits = null_mut();
            let bitmap = Bitmap(CreateDIBSection(
                memory.0,
                &bitmap_info(size),
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            ));
            if memory.0.is_null() || bitmap.0.is_null() || bits.is_null() {
                return Err(win_error("Cannot allocate window capture"));
            }
            let old = SelectObject(memory.0, bitmap.0);
            let ok = windows_sys::Win32::Storage::Xps::PrintWindow(hwnd, memory.0, 2);
            GdiFlush();
            SelectObject(memory.0, old);
            if ok == 0 {
                return Err(
                    "This window could not provide its image. Restore it and try again.".into(),
                );
            }
            let pixels = std::slice::from_raw_parts(
                bits.cast::<u8>(),
                size[0] as usize * size[1] as usize * 4,
            )
            .to_vec();
            Ok(Self {
                origin: [bounds.left, bounds.top],
                size,
                pixels,
                dimmed: vec![],
                cursor: if include_cursor {
                    cursor::current()?
                } else {
                    None
                },
            })
        }
    }
    fn frame(&self, region: Region) -> Result<capture_core::step_capture::Frame, String> {
        let image = crop_bgra(&self.pixels, self.size, region)?;
        let origin = [
            self.origin[0] + region.x as i32,
            self.origin[1] + region.y as i32,
        ];
        let cursor = self
            .cursor
            .as_ref()
            .map(|c| c.annotation(origin, [region.width, region.height], 1))
            .transpose()
            .map_err(|e| e.to_string())?
            .flatten();
        Ok(capture_core::step_capture::Frame {
            image,
            cursor,
            context: Default::default(),
        })
    }
    fn document(&self, region: Region) -> Result<Document, String> {
        let frame = self.frame(region)?;
        let mut document = Document::from_capture_image(frame.image).map_err(|e| e.to_string())?;
        if let Some(mut cursor) = frame.cursor {
            cursor.id = 2;
            document.annotations.push(cursor);
            document.promote_features();
        }
        document.validate().map_err(|e| e.to_string())?;
        Ok(document)
    }
    fn capture(include_cursor: bool) -> Result<Self, String> {
        // SAFETY: Win32 handles are checked, deselected before readback/deletion,
        // and owned by RAII guards. Allocation bounds are checked before GDI.
        unsafe {
            let origin = [
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
            ];
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            if width <= 0
                || height <= 0
                || width > 32768
                || height > 32768
                || i64::from(width) * i64::from(height) > 64_000_000
            {
                return Err(
                    "Desktop bounds are unavailable or exceed the 64 megapixel capture limit."
                        .into(),
                );
            }
            Self::capture_rect(origin, [width as u32, height as u32], include_cursor, true)
        }
    }
    fn capture_rect(
        origin: [i32; 2],
        size: [u32; 2],
        include_cursor: bool,
        dim: bool,
    ) -> Result<Self, String> {
        if size.contains(&0)
            || size.iter().any(|n| *n > 32768)
            || size[0] as u64 * size[1] as u64 > 64_000_000
        {
            return Err("Capture bounds exceed limits".into());
        }
        let [width, height] = size.map(|n| n as i32);
        unsafe {
            let screen = ScreenDc(GetDC(null_mut()));
            if screen.0.is_null() {
                return Err(win_error("Cannot access the desktop"));
            }
            let memory = MemoryDc(CreateCompatibleDC(screen.0));
            if memory.0.is_null() {
                return Err(win_error("Cannot create capture surface"));
            }
            let bitmap = Bitmap(CreateCompatibleBitmap(screen.0, width, height));
            if bitmap.0.is_null() {
                return Err(win_error("Cannot allocate capture bitmap"));
            }
            let previous = SelectObject(memory.0, bitmap.0);
            if previous.is_null() || previous as isize == -1 {
                return Err(win_error("Cannot select capture bitmap"));
            }
            let captured = BitBlt(
                memory.0,
                0,
                0,
                width,
                height,
                screen.0,
                origin[0],
                origin[1],
                SRCCOPY | CAPTUREBLT,
            );
            SelectObject(memory.0, previous);

            if captured == 0 {
                return Err(win_error("Windows screen capture failed"));
            }
            let mut pixels = vec![0; width as usize * height as usize * 4];
            let mut info = bitmap_info(size);
            if GetDIBits(
                screen.0,
                bitmap.0,
                0,
                height as u32,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            ) != height
            {
                return Err(win_error("Cannot read captured pixels"));
            }
            let dimmed = if dim {
                pixels.iter().map(|v| v / 2).collect()
            } else {
                Vec::new()
            };
            Ok(Self {
                origin,
                size,
                pixels,
                dimmed,
                cursor: if include_cursor {
                    cursor::current()?
                } else {
                    None
                },
            })
        }
    }
}

struct Picker {
    snapshot: Snapshot,
    start: Option<[i32; 2]>,
    pointer: [i32; 2],
    selection: Option<Region>,
    finished: bool,
    windows: Vec<HWND>,
    window_targets: Option<Vec<[i32; 4]>>,
}

impl Picker {
    fn region(&self) -> Option<Region> {
        if let Some(targets) = &self.window_targets {
            return Region::window_at(
                self.snapshot.origin,
                self.snapshot.size,
                self.pointer,
                targets,
            );
        }
        self.start.and_then(|start| {
            Region::from_drag(
                self.snapshot.origin,
                self.snapshot.size,
                start,
                self.pointer,
            )
        })
    }

    fn pointer_event(&mut self, point: [i32; 2], pressed: bool, released: bool) -> bool {
        let old_region = self.region();
        self.pointer = point;
        if pressed {
            self.start = Some(point);
        }
        if released {
            self.selection = self.region();
            self.finished = self.selection.is_some();
            self.start = None;
        }
        repaint_selection(self.window_targets.is_some(), old_region, self.region())
    }

    fn paint_into(&self, dc: HDC, origin: [i32; 2], size: [i32; 2], dpi: u32) {
        // Immutable original buffers are never painted into, including the lens.
        unsafe {
            let [width, height] = self.snapshot.size.map(|v| v as i32);
            let offset_x = origin[0] - self.snapshot.origin[0];
            let offset_y = origin[1] - self.snapshot.origin[1];
            let info = bitmap_info(self.snapshot.size);
            let draw = |pixels: &[u8]| {
                StretchDIBits(
                    dc,
                    -offset_x,
                    -offset_y,
                    width,
                    height,
                    0,
                    0,
                    width,
                    height,
                    pixels.as_ptr().cast(),
                    &info,
                    DIB_RGB_COLORS,
                    SRCCOPY,
                )
            };
            draw(&self.snapshot.dimmed);
            let region = self.region();
            if let Some(region) = region {
                let rect = RECT {
                    left: region.x as i32 - offset_x,
                    top: region.y as i32 - offset_y,
                    right: (region.x + region.width) as i32 - offset_x,
                    bottom: (region.y + region.height) as i32 - offset_y,
                };
                let saved = SaveDC(dc);
                IntersectClipRect(dc, rect.left, rect.top, rect.right, rect.bottom);
                draw(&self.snapshot.pixels);
                RestoreDC(dc, saved);
                if self.window_targets.is_some() {
                    let brush = Bitmap(CreateSolidBrush(0x0045f5bd));
                    FrameRect(dc, &rect, brush.0);
                } else {
                    region_preview::dashed(
                        dc,
                        &[
                            [rect.left, rect.top],
                            [rect.right, rect.top],
                            [rect.right, rect.bottom],
                            [rect.left, rect.bottom],
                            [rect.left, rect.top],
                        ],
                    );
                }
            }
            if self.window_targets.is_none() {
                region_preview::paint(dc, self, origin, size, dpi);
                return;
            }
            let text = match region {
                Some(r) if self.window_targets.is_some() => format!(
                    "{} × {} px  ·  Click window to capture  ·  Esc / right-click to cancel",
                    r.width, r.height
                ),
                Some(r) => format!(
                    "{} × {} px  ·  Release to capture  ·  Esc / right-click to cancel",
                    r.width, r.height
                ),
                None => if self.window_targets.is_some() {
                    "Point at a window to capture  ·  Esc / right-click to cancel"
                } else {
                    "Drag to capture a region  ·  Esc / right-click to cancel"
                }
                .into(),
            };
            let wide: Vec<u16> = text.encode_utf16().collect();
            let anchor = if self.window_targets.is_some() {
                region
                    .map(|r| {
                        [
                            self.snapshot.origin[0] + r.x as i32,
                            self.snapshot.origin[1] + r.y as i32,
                        ]
                    })
                    .unwrap_or([origin[0], origin[1]])
            } else {
                self.pointer
            };
            let x = (anchor[0] - origin[0] + 18).clamp(8, (size[0] - 580).max(8));
            let y = (anchor[1] - origin[1] + 24).clamp(8, (size[1] - 40).max(8));
            let label = RECT {
                left: x - 6,
                top: y - 6,
                right: (x + 570).min(size[0]),
                bottom: y + 25,
            };
            SetBkColor(dc, 0x00231c18);
            SetTextColor(dc, 0x00ffffff);
            ExtTextOutW(
                dc,
                x,
                y,
                ETO_OPAQUE,
                &label,
                wide.as_ptr(),
                wide.len() as u32,
                null(),
            );
        }
    }

    fn paint(&self, hwnd: HWND) {
        unsafe {
            let mut paint = PAINTSTRUCT::default();
            let front = BeginPaint(hwnd, &mut paint);
            if !front.is_null() {
                let mut origin = POINT::default();
                ClientToScreen(hwnd, &mut origin);
                let mut client = RECT::default();
                GetClientRect(hwnd, &mut client);
                let buffer = MemoryDc(CreateCompatibleDC(front));
                let bitmap = Bitmap(CreateCompatibleBitmap(front, client.right, client.bottom));
                if !buffer.0.is_null() && !bitmap.0.is_null() {
                    let previous = SelectObject(buffer.0, bitmap.0);
                    self.paint_into(
                        buffer.0,
                        [origin.x, origin.y],
                        [client.right, client.bottom],
                        GetDpiForWindow(hwnd),
                    );
                    BitBlt(
                        front,
                        0,
                        0,
                        client.right,
                        client.bottom,
                        buffer.0,
                        0,
                        0,
                        SRCCOPY,
                    );
                    SelectObject(buffer.0, previous);
                }
            }
            EndPaint(hwnd, &paint);
        }
    }
}

// SAFETY: GWLP_USERDATA points to the boxed Picker on this same thread, from
// WM_NCCREATE until WM_NCDESTROY. Mutable borrows end before reentrant calls.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            // Let Windows initialize the caption and normal window metadata.
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        if message == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Picker;
        if pointer.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        match message {
            WM_PAINT => {
                (&*pointer).paint(hwnd);
                0
            }
            WM_ERASEBKGND => 1,
            WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
                // Use each event's coordinates. GetCursorPos can already be at
                // the release position while a fast drag's down event is queued.
                let mut point = POINT {
                    x: lparam as u16 as i16 as i32,
                    y: (lparam >> 16) as u16 as i16 as i32,
                };
                if ClientToScreen(hwnd, &mut point) == 0 {
                    return 0;
                }
                #[cfg(debug_assertions)]
                if message != WM_MOUSEMOVE && std::env::var_os("CLIPCRAFT_CAPTURE_TRACE").is_some()
                {
                    eprintln!(
                        "capture-input message={message} point={},{} origin={:?} size={:?}",
                        point.x,
                        point.y,
                        (*pointer).snapshot.origin,
                        (*pointer).snapshot.size
                    );
                }
                let repaint = (&mut *pointer).pointer_event(
                    [point.x, point.y],
                    message == WM_LBUTTONDOWN,
                    message == WM_LBUTTONUP,
                );
                if message == WM_LBUTTONDOWN {
                    SetCapture(hwnd);
                }
                if message == WM_LBUTTONUP {
                    ReleaseCapture();
                }
                if repaint {
                    for &overlay in &(*pointer).windows {
                        InvalidateRect(overlay, null(), 0);
                    }
                }
                0
            }
            WM_KEYDOWN if wparam == VK_ESCAPE as usize => {
                (*pointer).finished = true;
                0
            }
            WM_RBUTTONDOWN | WM_CLOSE | WM_DISPLAYCHANGE => {
                (*pointer).finished = true;
                0
            }
            WM_ACTIVATEAPP if wparam == 0 => {
                (*pointer).finished = true;
                0
            }
            WM_CAPTURECHANGED if (*pointer).start.is_some() => {
                (*pointer).finished = true;
                0
            }
            WM_DPICHANGED => 0, // Physical virtual-desktop bounds intentionally stay fixed.
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

unsafe extern "system" fn collect_monitor(
    _: HMONITOR,
    _: HDC,
    bounds: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // EnumDisplayMonitors invokes this synchronously while the vector is alive.
    unsafe { (&mut *(data as *mut Vec<RECT>)).push(*bounds) };
    1
}

impl Drop for Picker {
    fn drop(&mut self) {
        // Disconnect callbacks before destruction, which may synchronously send
        // activation/capture messages while Drop holds a mutable Picker borrow.
        for hwnd in self.windows.drain(..) {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DestroyWindow(hwnd);
            }
        }
    }
}

pub(super) fn select_image(
    owner: isize,
    settings: capture_core::capture_options::Settings,
    history_root: Option<std::path::PathBuf>,
    scroll_mode: std::sync::mpsc::Sender<bool>,
    video: Option<super::VideoRequest>,
) -> CaptureResult {
    settings.request().map_err(|e| e.to_string())?;
    if video.is_some()
        && !matches!(
            settings.selection,
            capture_core::capture_options::Selection::Region
                | capture_core::capture_options::Selection::Window
                | capture_core::capture_options::Selection::Fullscreen
        )
    {
        return Err("Choose Region, Window or Fullscreen for video.".into());
    }
    unsafe {
        let previous_dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if previous_dpi.is_null() {
            return Err(win_error("Cannot enable physical-pixel capture"));
        }
        let _dpi = DpiContext(previous_dpi);
        let owner = owner as HWND;
        let _restore = RestoreEditor(owner, IsWindowVisible(owner) != 0);
        hide_for_capture(owner)?;
        if !countdown::wait(owner, settings.delay_seconds)? {
            return Ok(None);
        }
        // Let the compositor remove the editor before freezing the desktop.
        DwmFlush();
        std::thread::sleep(Duration::from_millis(160));
        DwmFlush();
        if settings.selection == capture_core::capture_options::Selection::Steps {
            return step_recorder::record(owner, settings.cursor, history_root);
        }
        let snapshot = Snapshot::capture(settings.cursor)?;
        if settings.selection == capture_core::capture_options::Selection::Fullscreen {
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor);
            let available = displays()?;
            let target = if settings.fullscreen == capture_core::capture_options::Fullscreen::Ask {
                let Some(device) = monitor_picker::choose(owner, &snapshot, &available)? else {
                    return Ok(None);
                };
                capture_core::capture_options::Fullscreen::Display(device)
            } else {
                settings.fullscreen.clone()
            };
            let bounds = capture_core::capture_options::fullscreen_bounds(
                &target,
                &displays()?,
                [cursor.x, cursor.y],
            )
            .map_err(|e| e.to_string())?;
            let region = Region::from_drag(
                snapshot.origin,
                snapshot.size,
                [bounds[0], bounds[1]],
                [bounds[2], bounds[3]],
            )
            .ok_or("Display is outside the captured desktop")?;
            if let Some(video) = video {
                drop(snapshot);
                DwmFlush();
                std::thread::sleep(Duration::from_millis(160));
                return video_recording::record(
                    owner,
                    [bounds[0], bounds[1]],
                    [region.width, region.height],
                    None,
                    video,
                );
            }
            if settings.is_scrolling() {
                drop(snapshot);
                DwmFlush();
                return scrolling::record(
                    owner,
                    [bounds[0], bounds[1]],
                    [region.width, region.height],
                    settings.scroll_automatic,
                    scroll_mode,
                );
            }
            return snapshot.document(region).map(|d| Some(d.into()));
        }
        let window_targets =
            if settings.selection == capture_core::capture_options::Selection::Window {
                let targets = window_targets();
                if targets.is_empty() {
                    return Err("No visible application window is available to capture".into());
                }
                Some(targets)
            } else {
                None
            };
        static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                let class = WNDCLASSW {
                    hIcon: crate::brand::native_icon(),
                    lpfnWndProc: Some(window_proc),
                    hInstance: GetModuleHandleW(null()),
                    hCursor: LoadCursorW(null_mut(), IDC_CROSS),
                    lpszClassName: w!("ClipCraftRegionPicker"),
                    ..Default::default()
                };
                let atom = RegisterClassW(&class);
                if atom == 0 {
                    Err(win_error("Cannot register capture window"))
                } else {
                    Ok(atom)
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor);
        let mut picker = Box::new(Picker {
            snapshot,
            window_targets,
            start: None,
            pointer: [cursor.x, cursor.y],
            selection: None,
            finished: false,
            windows: Vec::new(),
        });
        let mut monitors = Vec::<RECT>::new();
        if EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect_monitor),
            (&mut monitors as *mut Vec<RECT>) as LPARAM,
        ) == 0
            || monitors.is_empty()
        {
            return Err(win_error("Cannot enumerate capture displays"));
        }
        let mut active: HWND = null_mut();
        for bounds in monitors {
            // One physical-pixel window per monitor avoids oversized-window
            // scaling in accessibility/window navigation and mixed-DPI systems.
            // All overlays share the frozen desktop and one selection state.
            let title: Vec<u16> = format!(
                "ClipCraft — Select region ({}, {})\0",
                bounds.left, bounds.top
            )
            .encode_utf16()
            .collect();
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST,
                w!("ClipCraftRegionPicker"),
                title.as_ptr(),
                WS_POPUP,
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                (&mut *picker as *mut Picker).cast(),
            );
            if hwnd.is_null() {
                return Err(win_error("Cannot open region selection"));
            }
            picker.windows.push(hwnd);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            if active.is_null()
                || (cursor.x >= bounds.left
                    && cursor.x < bounds.right
                    && cursor.y >= bounds.top
                    && cursor.y < bounds.bottom)
            {
                active = hwnd;
            }
        }
        SetForegroundWindow(active);
        SetFocus(active);
        let mut message = MSG::default();
        let mut failed = false;
        loop {
            // DispatchMessage calls window_proc, which updates this same Picker.
            if picker.finished {
                break;
            }
            let status = GetMessageW(&mut message, null_mut(), 0, 0);
            if status <= 0 {
                failed = status < 0;
                break;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if failed {
            return Err("The Windows capture event loop failed.".into());
        }
        let region = picker.selection;
        if settings.is_scrolling() && video.is_none() {
            let origin = picker.snapshot.origin;
            drop(picker);
            DwmFlush();
            std::thread::sleep(Duration::from_millis(160));
            return match region {
                Some(r) => scrolling::record(
                    owner,
                    [origin[0] + r.x as i32, origin[1] + r.y as i32],
                    [r.width, r.height],
                    settings.scroll_automatic,
                    scroll_mode,
                ),
                None => Ok(None),
            };
        }
        if let Some(video) = video {
            let origin = picker.snapshot.origin;
            let point = picker.pointer;
            drop(picker);
            DwmFlush();
            std::thread::sleep(Duration::from_millis(160));
            return match region {
                Some(r) => video_recording::record(
                    owner,
                    [origin[0] + r.x as i32, origin[1] + r.y as i32],
                    [r.width, r.height],
                    (settings.selection == capture_core::capture_options::Selection::Window).then(
                        || {
                            GetAncestor(
                                WindowFromPoint(POINT {
                                    x: point[0],
                                    y: point[1],
                                }),
                                GA_ROOT,
                            ) as usize
                        },
                    ),
                    video,
                ),
                None => Ok(None),
            };
        }
        if settings.selection == capture_core::capture_options::Selection::Window
            && let Some(region) = region
        {
            let origin = picker.snapshot.origin;
            let point = picker.pointer;
            drop(picker);
            DwmFlush();
            let target = GetAncestor(
                WindowFromPoint(POINT {
                    x: point[0],
                    y: point[1],
                }),
                GA_ROOT,
            );
            let snapshot = Snapshot::capture_window(target, settings.cursor)?;
            let crop = Region::from_drag(
                snapshot.origin,
                snapshot.size,
                [origin[0] + region.x as i32, origin[1] + region.y as i32],
                [
                    origin[0] + region.x as i32 + region.width as i32,
                    origin[1] + region.y as i32 + region.height as i32,
                ],
            )
            .ok_or("The selected window moved. Capture it again.")?;
            return snapshot.document(crop).map(|d| Some(d.into()));
        }
        // Region capture retains its frozen screen snapshot.
        match region {
            Some(region) => picker.snapshot.document(region).map(|d| Some(d.into())),
            None => Ok(None),
        }
    }
}

// Walk the explicit Z order. Windows can close/reorder during enumeration, so
// cap traversal and detect repeated handles. Keep geometry only, never titles.
unsafe fn window_targets() -> Vec<[i32; 4]> {
    unsafe {
        let mut targets = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut hwnd = GetTopWindow(null_mut());
        while !hwnd.is_null() && seen.len() < 4096 && seen.insert(hwnd as usize) {
            if let Some(bounds) = window_bounds(hwnd) {
                targets.push(bounds);
            }
            hwnd = GetWindow(hwnd, GW_HWNDNEXT);
        }
        targets
    }
}

unsafe fn window_bounds(hwnd: HWND) -> Option<[i32; 4]> {
    unsafe {
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, &mut process);
        if process == std::process::id() {
            return None;
        }
        if IsWindowVisible(hwnd) == 0
            || IsIconic(hwnd) != 0
            || hwnd == GetDesktopWindow()
            || hwnd == GetShellWindow()
            || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
        {
            return None;
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast(),
            4,
        ) == 0
            && cloaked != 0
        {
            return None;
        }
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut rect as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        ) != 0
            && GetWindowRect(hwnd, &mut rect) == 0
        {
            return None;
        }
        (rect.right > rect.left && rect.bottom > rect.top).then_some([
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
        ])
    }
}

pub(super) fn displays() -> Result<Vec<capture_core::capture_options::Display>, String> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        unsafe {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) == 0 {
                return 0;
            }
            let rect = info.monitorInfo.rcMonitor;
            (&mut *(data as *mut Vec<capture_core::capture_options::Display>)).push(
                capture_core::capture_options::Display {
                    device: String::from_utf16_lossy(
                        &info.szDevice[..info
                            .szDevice
                            .iter()
                            .position(|c| *c == 0)
                            .unwrap_or(info.szDevice.len())],
                    ),
                    bounds: [rect.left, rect.top, rect.right, rect.bottom],
                    primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                },
            );
            1
        }
    }
    unsafe {
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if previous.is_null() {
            return Err(win_error("Cannot read physical display bounds"));
        }
        let _dpi = DpiContext(previous);
        let mut displays = Vec::new();
        if EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect),
            (&mut displays as *mut Vec<capture_core::capture_options::Display>) as LPARAM,
        ) == 0
        {
            return Err(win_error("Cannot list displays"));
        }
        Ok(displays)
    }
}

#[cfg(test)]
mod document_tests {
    use super::*;
    #[test]
    fn capture_crop_keeps_screenshot_and_cursor_as_separate_editable_objects() {
        let pixels = [30, 80, 170, 255].repeat(40 * 30);
        let cursor_pixels = image::RgbaImage::from_pixel(3, 4, image::Rgba([255; 4]));
        let cursor = capture_core::captured_cursor::Cursor::from_backgrounds(
            [14, 16],
            &cursor_pixels,
            &cursor_pixels,
        );
        let snapshot = Snapshot {
            origin: [0, 0],
            size: [40, 30],
            dimmed: pixels.clone(),
            pixels,
            cursor: Some(cursor.unwrap()),
        };
        let document = snapshot
            .document(Region {
                x: 10,
                y: 10,
                width: 20,
                height: 15,
            })
            .unwrap();
        assert_eq!(document.annotations.len(), 2);
        assert_eq!(document.annotations[0].id, 1);
        assert!(!document.annotations[0].image.as_ref().unwrap().is_cursor());
        assert_eq!(document.annotations[1].id, 2);
        assert!(document.annotations[1].image.as_ref().unwrap().is_cursor());
        assert_eq!(document.annotations[1].start, [4., 6.]);
        let mut session = capture_core::Session::new(document).unwrap();
        session
            .execute(capture_core::Command::Remove { id: 1 })
            .unwrap();
        assert_eq!(session.document.annotations.len(), 1);
        assert_eq!(session.document.render().unwrap().get_pixel(0, 0)[3], 0);
        session.execute(capture_core::Command::Undo).unwrap();
        assert_eq!(
            session.document.render().unwrap().get_pixel(0, 0).0,
            [170, 80, 30, 255]
        );
    }
}

#[cfg(test)]
mod hide_tests {
    use super::*;
    #[test]
    fn capture_hide_disables_transitions_on_owned_never_shown_window() {
        unsafe {
            let window = CreateWindowExW(
                0,
                w!("STATIC"),
                w!("ClipCraft capture visibility fixture"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                80,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!window.is_null());
            let result = hide_for_capture(window);
            let hidden = IsWindowVisible(window) == 0;
            DestroyWindow(window);
            assert!(result.is_ok(), "{result:?}");
            assert!(hidden);
        }
    }
}
