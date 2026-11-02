//! Explicit confirmation for destructive restart of the current recording only.
use super::*;
struct Dialog {
    result: Option<bool>,
    restart: bool,
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
                WM_COMMAND if [1, 2].contains(&(wparam & 0xffff)) => {
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
                        paint(dc, (*pointer).restart);
                        RestoreDC(dc, saved);
                    }
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                WM_DRAWITEM => {
                    let item =
                        &*(lparam as *const windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT);
                    let saved = SaveDC(item.hDC);
                    SetMapMode(item.hDC, MM_ANISOTROPIC);
                    SetWindowExtEx(
                        item.hDC,
                        104,
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
                            right: 104,
                            bottom: crate::controls::DIALOG_ACTION_HEIGHT as i32,
                        },
                        item.CtlID == 2,
                        item.itemState & 0x10 != 0,
                        (*pointer).restart,
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
        let x = area.left.clamp(
            info.rcWork.left,
            (info.rcWork.right - px(460)).max(info.rcWork.left),
        );
        let y = area.bottom.clamp(
            info.rcWork.top,
            (info.rcWork.bottom - px(160)).max(info.rcWork.top),
        );
        let mut state = Box::new(Dialog {
            result: None,
            restart: restarting,
            dpi,
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("ClipCraftRestartSteps"),
            w!("Restart step capture"),
            WS_POPUP | WS_CLIPCHILDREN,
            x,
            y,
            px(460),
            px(160),
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
            w!("Resume"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            px(222),
            px(112),
            px(104),
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
                w!("Restart")
            } else {
                w!("Delete")
            },
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            px(338),
            px(112),
            px(104),
            px(crate::controls::DIALOG_ACTION_HEIGHT as i32),
            hwnd,
            2usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        if resume.is_null() || restart.is_null() {
            return Err(win_error("Cannot create restart buttons"));
        }
        EnableWindow(owner, 0);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(resume);
        while state.result.is_none() {
            let mut msg = MSG::default();
            if GetMessageW(&mut msg, null_mut(), 0, 0) <= 0 {
                return Err("Restart confirmation closed unexpectedly; recording preserved".into());
            }
            if msg.message == WM_KEYDOWN && msg.wParam == VK_ESCAPE as usize {
                state.result = Some(false);
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
fn paint(dc: HDC, restart: bool) {
    unsafe {
        let brush = Bitmap(CreateSolidBrush(0x00413e38));
        let pen = Bitmap(CreatePen(PS_SOLID, 2, 0x00b98b00));
        let oldb = SelectObject(dc, brush.0);
        let oldp = SelectObject(dc, pen.0);
        RoundRect(dc, 0, 0, 460, 160, 12, 12);
        SelectObject(dc, oldp);
        SelectObject(dc, oldb);
        text(
            dc,
            if restart {
                "Restart capture?"
            } else {
                "Delete capture?"
            },
            -19,
            RECT {
                left: 18,
                top: 14,
                right: 442,
                bottom: 42,
            },
            DT_LEFT | DT_SINGLELINE,
        );
        text(
            dc,
            if restart {
                "Delete this recording and start again?\nThis action is permanent."
            } else {
                "Delete this recording?\nThis action is permanent."
            },
            -15,
            RECT {
                left: 18,
                top: 48,
                right: 442,
                bottom: 100,
            },
            DT_LEFT | DT_WORDBREAK,
        );
    }
}
fn button(dc: HDC, rect: RECT, destructive: bool, focus: bool, restart: bool) {
    unsafe {
        let brush = Bitmap(CreateSolidBrush(if destructive {
            0x00302ec1
        } else {
            0x0057524b
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
                if restart { "Restart" } else { "Delete" }
            } else {
                "Resume"
            },
            -15,
            rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_and_delete_confirmations_render_offscreen() {
        unsafe {
            for restart in [true, false] {
                let dc = MemoryDc(CreateCompatibleDC(null_mut()));
                let mut bits = null_mut();
                let bitmap = Bitmap(CreateDIBSection(
                    dc.0,
                    &bitmap_info([460, 160]),
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
                        left: 222,
                        top: 112,
                        right: 326,
                        bottom: 144,
                    },
                    false,
                    true,
                    restart,
                );
                button(
                    dc.0,
                    RECT {
                        left: 338,
                        top: 112,
                        right: 442,
                        bottom: 144,
                    },
                    true,
                    false,
                    restart,
                );
                GdiFlush();
                SelectObject(dc.0, old);
                let image = crop_bgra(
                    std::slice::from_raw_parts(bits.cast::<u8>(), 460 * 160 * 4),
                    [460, 160],
                    Region {
                        x: 0,
                        y: 0,
                        width: 460,
                        height: 160,
                    },
                )
                .unwrap();
                assert_eq!(image.get_pixel(344, 125).0, [193, 46, 48, 255]);
                assert_eq!(image.get_pixel(228, 125).0, [75, 82, 87, 255]);
                if let Some(dir) = std::env::var_os("CLIPCRAFT_CURSOR_EVIDENCE_DIR") {
                    image
                        .save(std::path::PathBuf::from(dir).join(if restart {
                            "restart-confirmation.png"
                        } else {
                            "delete-confirmation.png"
                        }))
                        .unwrap();
                }
            }
        }
    }
}
