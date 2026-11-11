//! Synthetic desktop surface for manual/Computer Use capture verification.
//! Launch before the editor. Escape closes it. It never inspects other windows.
#[cfg(windows)]
mod fixture {
    use std::{
        mem::size_of,
        ptr::{null, null_mut},
    };
    use windows_sys::{
        Win32::{
            Foundation::*,
            Graphics::Gdi::*,
            System::LibraryLoader::GetModuleHandleW,
            UI::{HiDpi::*, Input::KeyboardAndMouse::VK_ESCAPE, WindowsAndMessaging::*},
        },
        core::w,
    };

    struct Surface {
        width: i32,
        height: i32,
        pixels: Vec<u8>,
    }

    unsafe extern "system" fn procedure(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            if message == WM_NCCREATE {
                SetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                    (*(lparam as *const CREATESTRUCTW)).lpCreateParams as isize,
                );
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Surface;
            match message {
                WM_PAINT if !pointer.is_null() => {
                    let surface = &*pointer;
                    let mut paint = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut paint);
                    let info = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: surface.width,
                            biHeight: -surface.height,
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    StretchDIBits(
                        dc,
                        0,
                        0,
                        surface.width,
                        surface.height,
                        0,
                        0,
                        surface.width,
                        surface.height,
                        surface.pixels.as_ptr().cast(),
                        &info,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                    let title: Vec<u16> = format!("ClipDesigner synthetic capture fixture | {} x {} physical pixels | Escape closes this window", surface.width, surface.height).encode_utf16().collect();
                    SetTextColor(dc, 0xffffff);
                    SetBkColor(dc, 0x171717);
                    TextOutW(dc, 16, 16, title.as_ptr(), title.len() as i32);
                    EndPaint(hwnd, &paint);
                    0
                }
                WM_KEYDOWN if wparam == VK_ESCAPE as usize => {
                    DestroyWindow(hwnd);
                    0
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    0
                }
                _ => DefWindowProcW(hwnd, message, wparam, lparam),
            }
        }
    }

    pub fn run() {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            assert!(width > 0 && height > 0 && i64::from(width) * i64::from(height) <= 64_000_000);
            let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
            for y in 0..height {
                for x in 0..width {
                    pixels.extend_from_slice(&[
                        (((x / 64 + y / 64) % 2) * 160 + 32) as u8,
                        (y % 256) as u8,
                        (x % 256) as u8,
                        255,
                    ]);
                }
            }
            let surface = Surface {
                width,
                height,
                pixels,
            };
            let instance = GetModuleHandleW(null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: w!("ClipDesignerCaptureFixture"),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let hwnd = CreateWindowExW(
                0,
                class.lpszClassName,
                w!("ClipDesigner capture fixture"),
                WS_POPUP,
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                width,
                height,
                null_mut(),
                null_mut(),
                instance,
                (&surface as *const Surface).cast(),
            );
            assert!(!hwnd.is_null());
            ShowWindow(hwnd, SW_SHOW);
            let mut message = MSG::default();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

#[cfg(windows)]
fn main() {
    fixture::run();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("The capture fixture is Windows-only.");
}
