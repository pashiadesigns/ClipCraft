//! Private, in-memory monitor previews sampled before the picker is displayed.
use super::*;
use capture_core::capture_options::Display;
struct Card {
    device: String,
    label: String,
    size: [u32; 2],
    pixels: Vec<u8>,
}
struct State {
    cards: Vec<Card>,
    result: Option<Option<String>>,
    dpi: i32,
    page: usize,
    page_size: usize,
}
fn geometry(count: usize, work: [i32; 2], dpi: i32) -> Result<(usize, usize, [i32; 2]), String> {
    let px = |n: i32| n * dpi / 96;
    if work[0] < px(144) || work[1] < px(180) {
        return Err("Increase your display's available desktop area before choosing a monitor, or choose a fixed monitor in Clip settings.".into());
    }
    let columns = count.min(((work[0] - px(24)) / px(240)).clamp(1, 4) as usize);
    let rows = count
        .div_ceil(columns)
        .min(((work[1] - px(90)) / px(100)).max(1) as usize);
    Ok((
        columns,
        rows,
        [
            px(228).min((work[0] - px(24)) / columns as i32),
            px(160).min((work[1] - px(90)) / rows as i32),
        ],
    ))
}
unsafe fn show_page(hwnd: HWND, state: &State) {
    unsafe {
        for i in 0..state.cards.len() {
            ShowWindow(
                GetDlgItem(hwnd, (i + 1) as i32),
                if i / state.page_size == state.page {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_HIDE
                },
            );
        }
        EnableWindow(GetDlgItem(hwnd, 1001), (state.page > 0) as i32);
        EnableWindow(
            GetDlgItem(hwnd, 1002),
            ((state.page + 1) * state.page_size < state.cards.len()) as i32,
        );
        InvalidateRect(hwnd, null(), 0);
    }
}
unsafe fn text(dc: HDC, value: &str, dpi: i32, mut rect: RECT) {
    unsafe {
        let font = Bitmap(CreateFontW(
            -13 * dpi / 96,
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
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32,
            w!("Segoe UI"),
        ));
        let old = SelectObject(dc, font.0);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00ffffff);
        let mut text: Vec<u16> = value.encode_utf16().collect();
        DrawTextW(
            dc,
            text.as_mut_ptr(),
            text.len() as i32,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        SelectObject(dc, old);
    }
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
            let state = &mut *ptr;
            match msg {
                WM_CLOSE => {
                    state.result = Some(None);
                    return 0;
                }
                WM_COMMAND => {
                    let id = w & 0xffff;
                    if id == 1001 || id == 1002 {
                        if id == 1001 {
                            state.page = state.page.saturating_sub(1);
                        } else if (state.page + 1) * state.page_size < state.cards.len() {
                            state.page += 1;
                        }
                        show_page(hwnd, state);
                        SetFocus(GetDlgItem(hwnd, (state.page * state.page_size + 1) as i32));
                        return 0;
                    }
                    let index = (w & 0xffff).wrapping_sub(1);
                    if let Some(card) = state.cards.get(index) {
                        state.result = Some(Some(card.device.clone()));
                    }
                    return 0;
                }
                WM_ERASEBKGND => return 1,
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let mut r = RECT::default();
                    GetClientRect(hwnd, &mut r);
                    let brush = Bitmap(CreateSolidBrush(0x0033302b));
                    let pen = Bitmap(CreatePen(PS_SOLID, (2 * state.dpi / 96).max(1), 0x00c39a19));
                    let b = SelectObject(dc, brush.0);
                    let p = SelectObject(dc, pen.0);
                    RoundRect(
                        dc,
                        1,
                        1,
                        r.right - 1,
                        r.bottom - 1,
                        16 * state.dpi / 96,
                        16 * state.dpi / 96,
                    );
                    SelectObject(dc, p);
                    SelectObject(dc, b);
                    r.bottom = 40 * state.dpi / 96;
                    text(
                        dc,
                        "Choose a monitor to capture · Esc to cancel",
                        state.dpi,
                        r,
                    );
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                WM_DRAWITEM => {
                    let item = &*(l as *const windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT);
                    if item.CtlID >= 1001 {
                        let brush = Bitmap(CreateSolidBrush(0x0047433e));
                        FillRect(item.hDC, &item.rcItem, brush.0);
                        text(
                            item.hDC,
                            if item.CtlID == 1001 {
                                "Previous"
                            } else {
                                "Next"
                            },
                            state.dpi,
                            item.rcItem,
                        );
                        return 1;
                    }
                    if let Some(card) = state.cards.get(item.CtlID as usize - 1) {
                        let r = item.rcItem;
                        let pad = 6 * state.dpi / 96;
                        let footer = 28 * state.dpi / 96;
                        let brush = Bitmap(CreateSolidBrush(0x00211e1b));
                        FillRect(item.hDC, &r, brush.0);
                        let scale = ((r.right - r.left - 2 * pad) as f32 / card.size[0] as f32)
                            .min(
                                (r.bottom - r.top - footer - 2 * pad) as f32 / card.size[1] as f32,
                            );
                        let width = (card.size[0] as f32 * scale) as i32;
                        let height = (card.size[1] as f32 * scale) as i32;
                        SetStretchBltMode(item.hDC, HALFTONE);
                        StretchDIBits(
                            item.hDC,
                            (r.right - width) / 2,
                            pad + (r.bottom - footer - 2 * pad - height) / 2,
                            width,
                            height,
                            0,
                            0,
                            card.size[0] as i32,
                            card.size[1] as i32,
                            card.pixels.as_ptr().cast(),
                            &bitmap_info(card.size),
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        );
                        text(
                            item.hDC,
                            &card.label,
                            state.dpi,
                            RECT {
                                top: r.bottom - footer,
                                ..r
                            },
                        );
                        if item.itemState
                            & (windows_sys::Win32::UI::Controls::ODS_FOCUS
                                | windows_sys::Win32::UI::Controls::ODS_SELECTED)
                            != 0
                        {
                            let pen = Bitmap(CreatePen(PS_SOLID, 2 * state.dpi / 96, 0x00c39a19));
                            let p = SelectObject(item.hDC, pen.0);
                            let b = SelectObject(item.hDC, GetStockObject(HOLLOW_BRUSH));
                            RoundRect(item.hDC, 1, 1, r.right - 1, r.bottom - 1, 8, 8);
                            SelectObject(item.hDC, b);
                            SelectObject(item.hDC, p);
                        }
                    }
                    return 1;
                }
                _ => (),
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
struct Popup(HWND);
impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.0, GWLP_USERDATA, 0);
            DestroyWindow(self.0);
        }
    }
}
pub(super) fn choose(
    owner: HWND,
    snapshot: &Snapshot,
    displays: &[Display],
) -> Result<Option<String>, String> {
    unsafe {
        if displays.is_empty() || displays.len() > 32 {
            return Err(
                "Connect between 1 and 32 monitors, then try Fullscreen capture again.".into(),
            );
        }
        static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: GetModuleHandleW(null()),
                    hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                    lpszClassName: w!("ClipCraftMonitorPreview"),
                    ..Default::default()
                };
                let atom = RegisterClassW(&wc);
                if atom == 0 {
                    Err(win_error("Cannot create monitor preview"))
                } else {
                    Ok(atom)
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        let dpi = GetDpiForWindow(owner).max(96) as i32;
        let px = |n: i32| n * dpi / 96;
        let work = info.rcWork;
        let (columns, rows, [card_width, card_height]) = geometry(
            displays.len(),
            [work.right - work.left, work.bottom - work.top],
            dpi,
        )?;
        let width = card_width * columns as i32 + px(24);
        let paged = columns * rows < displays.len();
        let height = card_height * rows as i32 + px(if paged { 86 } else { 52 });
        let mut state = Box::new(State {
            cards: Vec::new(),
            result: None,
            dpi,
            page: 0,
            page_size: columns * rows,
        });
        for (index, display) in displays.iter().enumerate() {
            let b = display.bounds;
            let region =
                Region::from_drag(snapshot.origin, snapshot.size, [b[0], b[1]], [b[2], b[3]])
                    .ok_or("Monitor preview is outside the captured desktop")?;
            let thumb = image::imageops::thumbnail(
                &snapshot.frame(region)?.image,
                (card_width - px(12)).max(1) as u32,
                (card_height - px(40)).max(1) as u32,
            );
            let size = [thumb.width(), thumb.height()];
            let mut pixels = thumb.into_raw();
            for p in pixels.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
            state.cards.push(Card {
                device: display.device.clone(),
                label: format!(
                    "Monitor {} · {} × {}{}",
                    index + 1,
                    b[2] - b[0],
                    b[3] - b[1],
                    if display.primary { " · Main" } else { "" }
                ),
                size,
                pixels,
            });
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("ClipCraftMonitorPreview"),
            w!("Select monitor to capture"),
            WS_POPUP | WS_CLIPCHILDREN,
            work.left + (work.right - work.left - width) / 2,
            work.top + px(12),
            width,
            height,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            (&mut *state as *mut State).cast(),
        );
        if hwnd.is_null() {
            return Err(win_error("Cannot open monitor previews"));
        }
        let _window = Popup(hwnd);
        if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
            return Err(win_error("Cannot exclude monitor picker from capture"));
        }
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, px(16), px(16));
        if SetWindowRgn(hwnd, region, 0) == 0 {
            DeleteObject(region);
        }
        let mut first = null_mut();
        for (index, card) in state.cards.iter().enumerate() {
            let label: Vec<u16> = card.label.encode_utf16().chain(Some(0)).collect();
            let button = CreateWindowExW(
                0,
                w!("BUTTON"),
                label.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
                px(12) + (index % columns) as i32 * card_width,
                px(40) + ((index % state.page_size) / columns) as i32 * card_height,
                card_width - px(6),
                card_height - px(6),
                hwnd,
                (index + 1) as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            if button.is_null() {
                return Err(win_error("Cannot create monitor choice"));
            }
            if index == 0 {
                first = button;
            }
        }
        if paged {
            for (id, label, x) in [
                (1001, w!("Previous"), width - px(184)),
                (1002, w!("Next"), width - px(94)),
            ] {
                if CreateWindowExW(
                    0,
                    w!("BUTTON"),
                    label,
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
                    x,
                    height - px(38),
                    px(82),
                    px(26),
                    hwnd,
                    id as HMENU,
                    GetModuleHandleW(null()),
                    null(),
                )
                .is_null()
                {
                    return Err(win_error("Cannot create monitor navigation"));
                }
            }
        }
        show_page(hwnd, &state);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(first);
        while state.result.is_none() {
            let mut msg = MSG::default();
            if GetMessageW(&mut msg, null_mut(), 0, 0) <= 0 {
                return Ok(None);
            }
            if msg.message == WM_KEYDOWN && msg.wParam == VK_ESCAPE as usize {
                state.result = Some(None);
            } else if msg.message == WM_KEYDOWN && msg.wParam == VK_RETURN as usize {
                let id = GetDlgCtrlID(GetFocus());
                if id >= 1001 {
                    SendMessageW(hwnd, WM_COMMAND, id as usize, 0);
                } else if id > 0 {
                    state.result = state
                        .cards
                        .get(id as usize - 1)
                        .map(|c| Some(c.device.clone()));
                }
            } else if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(state.result.take().flatten())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_grid_paginates_with_readable_previews_at_all_display_scales() {
        for dpi in [96, 120, 144, 192, 384] {
            for count in 1..=32 {
                for work in [[1920, 1080], [1280, 1024], [1080, 1920]] {
                    let (cols, rows, card) = geometry(count, work, dpi).unwrap();
                    assert!(card[0] > 0 && card[1] >= 100 * dpi / 96);
                    assert!(cols * rows < count + cols);
                    assert!(cols as i32 * card[0] + 24 * dpi / 96 <= work[0]);
                    assert!(rows as i32 * card[1] + 86 * dpi / 96 <= work[1]);
                }
            }
        }
    }
}
