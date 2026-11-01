//! Keep Windows' native activation paint out of our completely custom frame.
use windows_sys::Win32::{
    Foundation::*,
    UI::{Shell::*, WindowsAndMessaging::*},
};
const SUBCLASS: usize = 0x43434652;
pub(super) fn install(owner: Option<isize>) {
    if let Some(owner) = owner {
        unsafe {
            SetWindowSubclass(owner as HWND, Some(procedure), SUBCLASS, 0);
        }
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
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
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
