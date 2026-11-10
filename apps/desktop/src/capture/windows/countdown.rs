//! Small owned countdown window. It is destroyed before any screenshot is taken.
use super::*;
use crate::capture::delay::{Countdown, State};
use std::time::Instant;

struct Dialog {
    countdown: Countdown,
    remaining: u32,
    hover: bool,
    result: Option<bool>,
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Dialog;
        if !state.is_null() {
            let state = &mut *state;
            match message {
                WM_CLOSE => {
                    state.result = Some(false);
                    return 0;
                }
                WM_ERASEBKGND => return 1,
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    if !dc.is_null() {
                        paint(dc, state.remaining, state.hover);
                    }
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                WM_MOUSEMOVE => {
                    let x = lparam as u16 as i16 as i32;
                    let y = (lparam >> 16) as u16 as i16 as i32;
                    let hover = (102..258).contains(&x) && (128..162).contains(&y);
                    if hover != state.hover {
                        state.hover = hover;
                        InvalidateRect(hwnd, null(), 0);
                    }
                    return 0;
                }
                WM_LBUTTONUP => {
                    let x = lparam as u16 as i16 as i32;
                    let y = (lparam >> 16) as u16 as i16 as i32;
                    if (102..258).contains(&x) && (128..162).contains(&y) {
                        state.result = Some(false);
                    }
                    return 0;
                }
                WM_KEYDOWN if wparam == VK_RETURN as usize || wparam == VK_SPACE as usize => {
                    state.result = Some(false);
                    return 0;
                }
                WM_KEYDOWN if wparam == VK_ESCAPE as usize => {
                    state.result = Some(false);
                    return 0;
                }
                WM_TIMER => {
                    match state
                        .countdown
                        .poll(Instant::now(), GetAsyncKeyState(VK_ESCAPE as i32) < 0)
                    {
                        State::Waiting(seconds) => {
                            if state.remaining != seconds {
                                state.remaining = seconds;
                                InvalidateRect(hwnd, null(), 0);
                            }
                        }
                        State::Ready => state.result = Some(true),
                        State::Cancelled => state.result = Some(false),
                    }
                    return 0;
                }
                _ => (),
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

pub(super) fn wait(owner: HWND, seconds: u32) -> Result<bool, String> {
    let countdown = Countdown::new(seconds, Instant::now())?;
    if seconds == 0 {
        return Ok(true);
    }
    // The boxed state outlives the message loop and every HWND. Clear its pointer
    // before destroying the parent, which also destroys both child controls.
    unsafe {
        static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                let class = WNDCLASSW {
                    hIcon: crate::brand::native_icon(),
                    lpfnWndProc: Some(procedure),
                    hInstance: GetModuleHandleW(null()),
                    hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                    hbrBackground: null_mut(),
                    lpszClassName: w!("ClipDesignerCaptureCountdown"),
                    ..Default::default()
                };
                let atom = RegisterClassW(&class);
                if atom == 0 {
                    Err(win_error("Cannot register countdown"))
                } else {
                    Ok(atom)
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let mut owner_rect = RECT::default();
        GetWindowRect(owner, &mut owner_rect);
        let mut state = Box::new(Dialog {
            countdown,
            remaining: seconds,
            hover: false,
            result: None,
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("ClipDesignerCaptureCountdown"),
            w!("ClipDesigner delayed capture"),
            WS_POPUP,
            owner_rect.left + 20,
            owner_rect.top + 30,
            360,
            184,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            (&mut *state as *mut Dialog).cast(),
        );
        if hwnd.is_null() {
            return Err(win_error("Cannot create countdown"));
        }
        let outcome = (|| {
            if SetTimer(hwnd, 1, 50, None) == 0 {
                return Err(win_error("Cannot initialize countdown"));
            }
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
            while state.result.is_none() {
                let mut message = MSG::default();
                let result = GetMessageW(&mut message, null_mut(), 0, 0);
                if result <= 0 {
                    return Err("Capture countdown stopped. Your project is unchanged.".into());
                }
                if IsDialogMessageW(hwnd, &message) == 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            Ok(state.result.unwrap_or(false))
        })();
        KillTimer(hwnd, 1);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        DestroyWindow(hwnd);
        outcome
    }
}

// The same renderer is exercised on offscreen pixels in tests; no app windows.
fn paint(dc: HDC, seconds: u32, hover: bool) {
    unsafe {
        let background = Bitmap(CreateSolidBrush(0x0023211d));
        let panel = Bitmap(CreateSolidBrush(0x00413e38));
        let blue = Bitmap(CreatePen(PS_SOLID, 2, 0x00b98b00));
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: 360,
                bottom: 184,
            },
            background.0,
        );
        let old_pen = SelectObject(dc, blue.0);
        let old_brush = SelectObject(dc, panel.0);
        RoundRect(dc, 1, 1, 359, 183, 18, 18);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00ffffff);
        let label = |text: &str, height: i32, mut rect: RECT| {
            let font = Bitmap(CreateFontW(
                height,
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
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                w!("Segoe UI"),
            ));
            let old = SelectObject(dc, font.0);
            let mut text: Vec<u16> = text.encode_utf16().collect();
            DrawTextW(
                dc,
                text.as_mut_ptr(),
                text.len() as i32,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            SelectObject(dc, old);
        };
        label(
            "ClipDesigner Capture",
            -17,
            RECT {
                left: 12,
                top: 12,
                right: 348,
                bottom: 38,
            },
        );
        label(
            &format!("{seconds}"),
            -42,
            RECT {
                left: 12,
                top: 38,
                right: 348,
                bottom: 92,
            },
        );
        SetTextColor(dc, 0x00dadada);
        label(
            "seconds remaining",
            -14,
            RECT {
                left: 12,
                top: 90,
                right: 348,
                bottom: 114,
            },
        );
        let button = Bitmap(CreateSolidBrush(if hover {
            0x00b98b00
        } else {
            0x0057524b
        }));
        SelectObject(dc, button.0);
        RoundRect(dc, 102, 128, 258, 162, 8, 8);
        SetTextColor(dc, 0x00ffffff);
        label(
            "Cancel · Esc",
            -15,
            RECT {
                left: 102,
                top: 128,
                right: 258,
                bottom: 162,
            },
        );
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn themed_countdown_renders_offscreen() {
        unsafe {
            let dc = MemoryDc(CreateCompatibleDC(null_mut()));
            let mut bits = null_mut();
            let bitmap = Bitmap(CreateDIBSection(
                dc.0,
                &bitmap_info([360, 184]),
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            ));
            assert!(!bitmap.0.is_null());
            let old = SelectObject(dc.0, bitmap.0);
            paint(dc.0, 3, false);
            GdiFlush();
            SelectObject(dc.0, old);
            let bytes = std::slice::from_raw_parts(bits.cast::<u8>(), 360 * 184 * 4);
            let image = crop_bgra(
                bytes,
                [360, 184],
                Region {
                    x: 0,
                    y: 0,
                    width: 360,
                    height: 184,
                },
            )
            .unwrap();
            assert_eq!(image.get_pixel(0, 0).0, [29, 33, 35, 255]);
            assert_ne!(image.get_pixel(180, 40).0, [255; 4]);
            if let Some(dir) = std::env::var_os("CLIPDESIGNER_CURSOR_EVIDENCE_DIR") {
                let path = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&path).unwrap();
                image.save(path.join("countdown.png")).unwrap();
            }
        }
    }
}
