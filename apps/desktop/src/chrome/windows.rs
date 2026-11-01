//! Keep Windows' native activation paint out of our completely custom frame.
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Dwm::*,
    UI::{Shell::*, WindowsAndMessaging::*},
};
const SUBCLASS: usize = 0x43434652;
pub(super) fn install(owner: Option<isize>) {
    if let Some(owner) = owner {
        unsafe {
            custom_frame(owner as HWND);
            SetWindowSubclass(owner as HWND, Some(procedure), SUBCLASS, 0);
        }
    }
}
unsafe fn custom_frame(hwnd: HWND) {
    // DWM's independent inactive-window border is not part of egui's client
    // surface. Suppress it for this custom-chrome window only; don't resize it.
    let color = 0xffff_fffe_u32;
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &color as *const _ as _, 4);
    }
}
fn geometry_event(message: u32) -> bool {
    matches!(
        message,
        WM_ACTIVATE | WM_SETFOCUS | WM_KILLFOCUS | WM_SIZE | WM_DPICHANGED
    )
}
unsafe fn record_geometry(hwnd: HWND, message: u32) {
    let path = std::env::var_os("CLIPCRAFT_FRAME_DIAGNOSTICS").map(std::path::PathBuf::from);
    let Some(path) = path else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 131072) {
        return;
    }
    let mut client = RECT::default();
    let mut outer = RECT::default();
    let mut visual = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut client);
        GetWindowRect(hwnd, &mut outer);
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut visual as *mut _ as _,
            std::mem::size_of::<RECT>() as u32,
        );
    }
    let rect = |r: RECT| [r.left, r.top, r.right, r.bottom];
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(
            f,
            "{}",
            serde_json::json!({"event":message,"client":rect(client),"outer":rect(outer),"dwm":rect(visual)})
        );
    }
}
fn activation_param(message: u32, minimized: bool, param: LPARAM) -> LPARAM {
    if message == WM_NCACTIVATE && !minimized {
        -1
    } else {
        param
    }
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    // Still pass the real activation message through winit, preserving focus and
    // keyboard state. Microsoft's -1 flag suppresses only DefWindowProc's frame paint.
    // https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-ncactivate
    let lparam = activation_param(message, unsafe { IsIconic(hwnd) } != 0, lparam);
    if message == WM_NCDESTROY {
        unsafe {
            RemoveWindowSubclass(hwnd, Some(procedure), id);
        }
    }
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    if geometry_event(message) {
        unsafe {
            custom_frame(hwnd);
            record_geometry(hwnd, message);
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_frame_suppresses_only_nonminimized_activation_paint() {
        assert_eq!(activation_param(WM_NCACTIVATE, false, 0), -1);
        assert_eq!(activation_param(WM_NCACTIVATE, true, 42), 42);
        for msg in [
            WM_KILLFOCUS,
            WM_SETFOCUS,
            WM_DPICHANGED,
            WM_SIZE,
            WM_NCDESTROY,
        ] {
            assert_eq!(activation_param(msg, false, 42), 42);
        }
    }
}
