//! Explicit confirmation for destructive restart of the current recording only.
use super::*;
struct Dialog {
    result: Option<bool>,
    restart: bool,
    steps: bool,
    dpi: i32,
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
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Dialog;
        if !pointer.is_null() {
            match message {
                WM_CLOSE => {
                    (*pointer).result = Some(false);
                    return 0;
                }
                WM_KEYDOWN if wparam == VK_ESCAPE as usize => {
                    (*pointer).result = Some(false);
                    return 0;
                }
                WM_COMMAND if [1, 2, 6].contains(&(wparam & 0xffff)) => {
                    (*pointer).result = Some(wparam & 0xffff == 2);
                    return 0;
                }
                WM_ERASEBKGND => return 1,
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    if !dc.is_null() {
                        let saved = SaveDC(dc);
                        SetMapMode(dc, MM_ANISOTROPIC);
                        SetWindowExtEx(dc, 96, 96, null_mut());
                        SetViewportExtEx(dc, (*pointer).dpi, (*pointer).dpi, null_mut());
                        paint_context(dc, (*pointer).restart, (*pointer).steps);
                        RestoreDC(dc, saved);
                    }
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                WM_DRAWITEM => {
                    let item =
                        &*(lparam as *const windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT);
                    if item.CtlID == 6 {
                        paint_close(
                            item.hDC,
                            [
                                item.rcItem.right - item.rcItem.left,
                                item.rcItem.bottom - item.rcItem.top,
                            ],
                        );
                        return 1;
                    }
                    let saved = SaveDC(item.hDC);
                    SetMapMode(item.hDC, MM_ANISOTROPIC);
                    SetWindowExtEx(
                        item.hDC,
                        82,
                        crate::controls::DIALOG_ACTION_HEIGHT as i32,
                        null_mut(),
                    );
                    SetViewportExtEx(
                        item.hDC,
                        item.rcItem.right - item.rcItem.left,
                        item.rcItem.bottom - item.rcItem.top,
                        null_mut(),
                    );
                    button(
                        item.hDC,
                        RECT {
                            left: 0,
                            top: 0,
                            right: 82,
                            bottom: crate::controls::DIALOG_ACTION_HEIGHT as i32,
                        },
                        item.CtlID == 2,
                        item.itemState & 0x10 != 0,
                        (*pointer).restart,
                        (*pointer).steps,
                    );
                    RestoreDC(item.hDC, saved);
                    return 1;
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
struct Popup(HWND, HWND);
impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.0, GWLP_USERDATA, 0);
            DestroyWindow(self.0);
            EnableWindow(self.1, 1);
            SetForegroundWindow(self.1);
        }
    }
}
pub(super) fn confirm(owner: HWND, restarting: bool) -> Result<bool, String> {
    confirm_context(owner, restarting, true)
}
pub(super) fn confirm_capture(owner: HWND, restarting: bool) -> Result<bool, String> {
    confirm_context(owner, restarting, false)
}
fn confirm_context(owner: HWND, restarting: bool, steps: bool) -> Result<bool, String> {
    unsafe {
        static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                let class = WNDCLASSW {
                    hIcon: crate::brand::native_icon(),
                    lpfnWndProc: Some(procedure),
                    hInstance: GetModuleHandleW(null()),
                    hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                    lpszClassName: w!("ClipCraftRestartSteps"),
                    ..Default::default()
                };
                let atom = RegisterClassW(&class);
                if atom == 0 {
                    Err(win_error("Cannot register restart confirmation"))
                } else {
                    Ok(atom)
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let mut area = RECT::default();
        GetWindowRect(owner, &mut area);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner).max(96) as i32;
        let px = |n: i32| (n * dpi + 48) / 96;
        let x = ((area.left + area.right - px(300)) / 2).clamp(
            info.rcWork.left,
            (info.rcWork.right - px(300)).max(info.rcWork.left),
        );
        let y = area.bottom.clamp(
            info.rcWork.top,
            (info.rcWork.bottom - px(132)).max(info.rcWork.top),
        );
        let mut state = Box::new(Dialog {
            result: None,
            restart: restarting,
            steps,
            dpi,
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("ClipCraftRestartSteps"),
            w!("Confirm capture action"),
            WS_POPUP | WS_CLIPCHILDREN,
            x,
            y,
            px(300),
            px(132),
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            (&mut *state as *mut Dialog).cast(),
        );
        if hwnd.is_null() {
            return Err(win_error("Cannot open restart confirmation"));
        }
        let _popup = Popup(hwnd, owner);
        if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
            return Err(win_error("Cannot exclude restart controls from capture"));
        }
        let resume = CreateWindowExW(
            0,
            w!("BUTTON"),
            if steps {
                w!("Resume")
            } else {
                w!("Keep capture")
            },
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            px(204),
            px(92),
            px(82),
            px(crate::controls::DIALOG_ACTION_HEIGHT as i32),
            hwnd,
            1usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        let restart = CreateWindowExW(
            0,
            w!("BUTTON"),
            if restarting {
                if steps { w!("Restart") } else { w!("Reset") }
            } else {
                w!("Discard")
            },
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            px(114),
            px(92),
            px(82),
            px(crate::controls::DIALOG_ACTION_HEIGHT as i32),
            hwnd,
            2usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        let close = CreateWindowExW(
            0,
            w!("BUTTON"),
            w!("Close"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            px(264),
            px(10),
            px(26),
            px(26),
            hwnd,
            6usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        let region = CreateRoundRectRgn(0, 0, px(300) + 1, px(132) + 1, px(16), px(16));
        if !region.is_null() && SetWindowRgn(hwnd, region, 1) == 0 {
            DeleteObject(region);
        }
        if resume.is_null() || restart.is_null() || close.is_null() {
            return Err(win_error("Cannot create restart buttons"));
        }
        EnableWindow(owner, 0);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(restart);
        while state.result.is_none() {
            let mut msg = MSG::default();
            if GetMessageW(&mut msg, null_mut(), 0, 0) <= 0 {
                return Err("Restart confirmation closed unexpectedly; recording preserved".into());
            }
            if msg.message == WM_KEYDOWN && msg.wParam == VK_ESCAPE as usize {
                state.result = Some(false);
            } else if msg.message == WM_KEYDOWN && msg.wParam == VK_RETURN as usize {
                state.result = Some(GetDlgCtrlID(GetFocus()) == 2);
            } else if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(state.result.unwrap_or(false))
    }
}
fn text(dc: HDC, value: &str, size: i32, mut rect: RECT, flags: u32) {
    unsafe {
        let font = Bitmap(CreateFontW(
            size,
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
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00ffffff);
        let mut value: Vec<u16> = value.encode_utf16().collect();
        DrawTextW(dc, value.as_mut_ptr(), value.len() as i32, &mut rect, flags);
        SelectObject(dc, old);
    }
}
#[cfg(test)]
fn paint(dc: HDC, restart: bool) {
    paint_context(dc, restart, true);
    // Compose the native child close button in the offscreen dialog fixture.
    unsafe {
        let saved = SaveDC(dc);
        OffsetViewportOrgEx(dc, 264, 8, null_mut());
        paint_close(dc, [26, 26]);
        RestoreDC(dc, saved);
    }
}
fn paint_context(dc: HDC, restart: bool, steps: bool) {
    supersampled(dc, [300, 132], |dc| paint_context_raw(dc, restart, steps));
}
fn paint_context_raw(dc: HDC, restart: bool, steps: bool) {
    unsafe {
        let brush = Bitmap(CreateSolidBrush(0x0033302b));
        let pen = Bitmap(CreatePen(PS_SOLID, 2, 0x00c39a19));
        let oldb = SelectObject(dc, brush.0);
        let oldp = SelectObject(dc, pen.0);
        RoundRect(dc, 0, 0, 300, 132, 16, 16);
        SelectObject(dc, oldp);
        SelectObject(dc, oldb);
        text(
            dc,
            if restart {
                if steps {
                    "Restart recording?"
                } else {
                    "Reset capture?"
                }
            } else {
                "Cancel capture?"
            },
            -16,
            RECT {
                left: 14,
                top: 14,
                right: 286,
                bottom: 42,
            },
            DT_LEFT | DT_SINGLELINE,
        );
        text(
            dc,
            if restart {
                if steps {
                    "Discard these steps and start again?"
                } else {
                    "Clear this capture and start again?"
                }
            } else {
                if steps {
                    "Discard these recorded steps?"
                } else {
                    "Discard the current capture?"
                }
            },
            -13,
            RECT {
                left: 14,
                top: 48,
                right: 286,
                bottom: 84,
            },
            DT_LEFT | DT_WORDBREAK,
        );
    }
}
fn button(dc: HDC, rect: RECT, destructive: bool, focus: bool, restart: bool, steps: bool) {
    unsafe {
        let saved = SaveDC(dc);
        OffsetViewportOrgEx(dc, rect.left, rect.top, null_mut());
        supersampled(dc, [rect.right - rect.left, rect.bottom - rect.top], |dc| {
            button_raw(
                dc,
                RECT {
                    left: 0,
                    top: 0,
                    right: rect.right - rect.left,
                    bottom: rect.bottom - rect.top,
                },
                destructive,
                focus,
                restart,
                steps,
            )
        });
        RestoreDC(dc, saved);
    }
}
fn button_raw(dc: HDC, rect: RECT, destructive: bool, focus: bool, restart: bool, steps: bool) {
    unsafe {
        let brush = Bitmap(CreateSolidBrush(if destructive {
            0x00c39a19
        } else {
            0x0047433e
        }));
        let pen = Bitmap(CreatePen(
            PS_SOLID,
            1,
            if focus { 0x00ffffff } else { 0x00736d66 },
        ));
        let oldb = SelectObject(dc, brush.0);
        let oldp = SelectObject(dc, pen.0);
        RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, 7, 7);
        SelectObject(dc, oldp);
        SelectObject(dc, oldb);
        text(
            dc,
            if destructive {
                if restart {
                    if steps { "Restart" } else { "Reset" }
                } else {
                    "Discard"
                }
            } else {
                if steps { "Resume" } else { "Keep capture" }
            },
            -13,
            rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}
fn supersampled(target: HDC, size: [i32; 2], paint: impl FnOnce(HDC)) {
    unsafe {
        let source = MemoryDc(CreateCompatibleDC(target));
        let mut bits = null_mut();
        let info = bitmap_info([(size[0] * 4) as u32, (size[1] * 4) as u32]);
        let bitmap = Bitmap(CreateDIBSection(
            source.0,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        ));
        if bitmap.0.is_null() {
            return;
        }
        let old = SelectObject(source.0, bitmap.0);
        SetMapMode(source.0, MM_ANISOTROPIC);
        SetWindowExtEx(source.0, 1, 1, null_mut());
        SetViewportExtEx(source.0, 4, 4, null_mut());
        paint(source.0);
        GdiFlush();
        let previous = SetStretchBltMode(target, HALFTONE);
        StretchDIBits(
            target,
            0,
            0,
            size[0],
            size[1],
            0,
            0,
            size[0] * 4,
            size[1] * 4,
            bits,
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        SetStretchBltMode(target, previous);
        SelectObject(source.0, old);
    }
}
fn paint_close(dc: HDC, size: [i32; 2]) {
    unsafe {
        let saved = SaveDC(dc);
        SetMapMode(dc, MM_ANISOTROPIC);
        SetWindowExtEx(dc, 26, 26, null_mut());
        SetViewportExtEx(dc, size[0], size[1], null_mut());
        supersampled(dc, [26, 26], |dc| {
            let brush = Bitmap(CreateSolidBrush(0x0033302b));
            FillRect(
                dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: 26,
                    bottom: 26,
                },
                brush.0,
            );
            let pen = Bitmap(CreatePen(PS_SOLID, 1, 0x00ffffff));
            let old = SelectObject(dc, pen.0);
            MoveToEx(dc, 8, 8, null_mut());
            LineTo(dc, 18, 18);
            MoveToEx(dc, 8, 18, null_mut());
            LineTo(dc, 18, 8);
            SelectObject(dc, old);
        });
        RestoreDC(dc, saved);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_and_cancel_confirmations_render_offscreen() {
        unsafe {
            for restart in [true, false] {
                let dc = MemoryDc(CreateCompatibleDC(null_mut()));
                let mut bits = null_mut();
                let bitmap = Bitmap(CreateDIBSection(
                    dc.0,
                    &bitmap_info([300, 132]),
                    DIB_RGB_COLORS,
                    &mut bits,
                    null_mut(),
                    0,
                ));
                assert!(!bitmap.0.is_null());
                let old = SelectObject(dc.0, bitmap.0);
                paint(dc.0, restart);
                button(
                    dc.0,
                    RECT {
                        left: 204,
                        top: 92,
                        right: 286,
                        bottom: 118,
                    },
                    false,
                    true,
                    restart,
                    true,
                );
                button(
                    dc.0,
                    RECT {
                        left: 114,
                        top: 92,
                        right: 196,
                        bottom: 118,
                    },
                    true,
                    false,
                    restart,
                    true,
                );
                GdiFlush();
                SelectObject(dc.0, old);
                let image = crop_bgra(
                    std::slice::from_raw_parts(bits.cast::<u8>(), 300 * 132 * 4),
                    [300, 132],
                    Region {
                        x: 0,
                        y: 0,
                        width: 300,
                        height: 132,
                    },
                )
                .unwrap();
                assert_eq!(image.get_pixel(120, 105).0, [25, 154, 195, 255]);
                assert_eq!(image.get_pixel(210, 105).0, [62, 67, 71, 255]);
                if let Some(dir) = std::env::var_os("CLIPCRAFT_CURSOR_EVIDENCE_DIR") {
                    image
                        .save(std::path::PathBuf::from(dir).join(if restart {
                            "restart-confirmation.png"
                        } else {
                            "cancel-confirmation.png"
                        }))
                        .unwrap();
                }
            }
        }
    }
}
