//! Non-activating, click-through startup cue. Excluded from captured screenshots.
use super::*;
use crate::capture::step_pipeline::Area;
use std::time::Instant;

pub(super) struct Overlay {
    hwnd: HWND,
    started: Instant,
}
impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}
impl Overlay {
    pub fn tick(&self) -> bool {
        let elapsed = self.started.elapsed().as_millis() as u64;
        unsafe {
            SetLayeredWindowAttributes(self.hwnd, 0, opacity(elapsed), LWA_ALPHA);
        }
        elapsed < 2000
    }
}
fn opacity(ms: u64) -> u8 {
    match ms {
        0..=160 => (ms * 190 / 160) as u8,
        161..=1000 => 190,
        1001..=1999 => ((2000 - ms) * 190 / 1000) as u8,
        _ => 0,
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_NCHITTEST => return HTTRANSPARENT as LRESULT,
            WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
            WM_ERASEBKGND => return 1,
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut rect = RECT::default();
                GetClientRect(hwnd, &mut rect);
                paint(dc, rect);
                EndPaint(hwnd, &ps);
                return 0;
            }
            _ => (),
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}
fn paint(dc: HDC, mut rect: RECT) {
    unsafe {
        let brush = Bitmap(CreateSolidBrush(0x00120f0c));
        FillRect(dc, &rect, brush.0);
        let font = Bitmap(CreateFontW(
            -32,
            0,
            0,
            0,
            600,
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
        SetTextColor(dc, 0x00ffffff);
        DrawTextW(
            dc,
            w!("Begin recording steps"),
            -1,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old);
    }
}
pub(super) fn show(owner: HWND, area: Area) -> Result<Overlay, String> {
    show_excluding(owner, area, |hwnd| unsafe {
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) != 0
    })
}
fn show_excluding(
    owner: HWND,
    area: Area,
    exclude: impl FnOnce(HWND) -> bool,
) -> Result<Overlay, String> {
    unsafe {
        static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                let class = WNDCLASSW {
                    hIcon: crate::brand::native_icon(),
                    lpfnWndProc: Some(procedure),
                    hInstance: GetModuleHandleW(null()),
                    lpszClassName: w!("ClipCraftStepsIntro"),
                    ..Default::default()
                };
                let atom = RegisterClassW(&class);
                if atom == 0 {
                    Err(win_error("Cannot register recording cue"))
                } else {
                    Ok(atom)
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("ClipCraftStepsIntro"),
            w!("Begin recording steps"),
            WS_POPUP | WS_DISABLED,
            area.origin[0],
            area.origin[1],
            area.size[0] as i32,
            area.size[1] as i32,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return Err(win_error("Cannot open recording cue"));
        }
        let overlay = Overlay {
            hwnd,
            started: Instant::now(),
        };
        if !exclude(hwnd) {
            return Err(win_error("Cannot exclude recording cue from capture"));
        }
        SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        Ok(overlay)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_optional_cue_is_destroyed_before_recording_can_continue() {
        let mut created = null_mut();
        let cue = show_excluding(
            null_mut(),
            Area {
                origin: [0, 0],
                size: [160, 80],
            },
            |hwnd| {
                created = hwnd;
                assert_eq!(unsafe { IsWindowVisible(hwnd) }, 0);
                unsafe {
                    SetLastError(ERROR_NOT_ENOUGH_MEMORY);
                }
                false
            },
        )
        .ok();
        assert!(cue.is_none());
        assert!(!created.is_null());
        assert_eq!(unsafe { IsWindow(created) }, 0);
    }
    #[test]
    fn brief_cue_fades_in_then_out_and_ends_at_two_seconds() {
        assert_eq!(opacity(0), 0);
        assert_eq!(opacity(80), 95);
        assert_eq!(opacity(160), 190);
        assert_eq!(opacity(1000), 190);
        assert_eq!(opacity(1500), 95);
        assert_eq!(opacity(2000), 0);
        assert_eq!(opacity(u64::MAX), 0);
    }
}
