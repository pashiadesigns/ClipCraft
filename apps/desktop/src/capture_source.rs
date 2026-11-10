//! Library > By app and By website: what was captured, read once at capture
//! and kept only in the local Library. Nothing is read for older captures.
use std::sync::Mutex;

/// The application, and a website when the window title names one.
pub(crate) type Source = (Option<String>, Option<String>);
static LAST: Mutex<Option<Source>> = Mutex::new(None);

pub(crate) fn remember(source: Option<Source>) {
    *LAST.lock().unwrap() = source;
}
pub(crate) fn take() -> Option<Source> {
    LAST.lock().unwrap().take()
}
/// A domain written in a window title, such as "Docs - example.com". Browsers
/// usually leave the address out of the title, so many captures have none.
pub(crate) fn site_in_title(title: &str) -> Option<String> {
    const FILES: [&str; 12] = [
        "png", "jpg", "jpeg", "gif", "txt", "pdf", "doc", "docx", "xls", "xlsx", "exe", "rs",
    ];
    title
        .split(|c: char| c.is_whitespace() || "|–—()[]".contains(c))
        .map(|t| {
            t.trim_start_matches("https://")
                .trim_start_matches("http://")
        })
        .map(|t| {
            t.split('/')
                .next()
                .unwrap_or("")
                .trim_matches(['.', ',', ':', '-'])
        })
        .map(|t| t.strip_prefix("www.").unwrap_or(t).to_ascii_lowercase())
        .find(|t| {
            let labels: Vec<_> = t.split('.').collect();
            let Some(last) = labels.last() else {
                return false;
            };
            labels.len() >= 2
                && last.len() >= 2
                && last.chars().all(|c| c.is_ascii_alphabetic())
                && !FILES.contains(last)
                && labels.iter().all(|l| {
                    !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                })
        })
}
fn app_name(stem: &str) -> String {
    let mut chars = stem.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The topmost window of another program under the pointer.
#[cfg(windows)]
pub(crate) fn under_cursor() -> Option<Source> {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_HWNDNEXT, GetCursorPos, GetTopWindow, GetWindow, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible,
    };
    // SAFETY: each call receives live out-parameters of the documented type and
    // size; the window list is only read; the process handle is closed once.
    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
        let mut hwnd: HWND = GetTopWindow(std::ptr::null_mut());
        while !hwnd.is_null() {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut cloaked = 0u32;
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED as u32,
                (&raw mut cloaked).cast(),
                size_of::<u32>() as u32,
            );
            if pid != GetCurrentProcessId()
                && IsWindowVisible(hwnd) != 0
                && cloaked == 0
                && GetWindowRect(hwnd, &mut rect) != 0
                && (rect.left..rect.right).contains(&point.x)
                && (rect.top..rect.bottom).contains(&point.y)
            {
                let mut title = [0u16; 512];
                let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0);
                let title = String::from_utf16_lossy(&title[..n as usize]);
                let mut app = None;
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if !process.is_null() {
                    let mut path = [0u16; 1024];
                    let mut size = path.len() as u32;
                    if QueryFullProcessImageNameW(
                        process,
                        PROCESS_NAME_WIN32,
                        path.as_mut_ptr(),
                        &mut size,
                    ) != 0
                    {
                        app =
                            std::path::Path::new(&String::from_utf16_lossy(&path[..size as usize]))
                                .file_stem()
                                .map(|s| app_name(&s.to_string_lossy()));
                    }
                    CloseHandle(process);
                }
                return Some((app, site_in_title(&title)));
            }
            hwnd = GetWindow(hwnd, GW_HWNDNEXT);
        }
    }
    None
}
#[cfg(not(windows))]
pub(crate) fn under_cursor() -> Option<Source> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_real_domains_in_a_title_count_as_a_website() {
        assert_eq!(
            site_in_title("Docs - example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            site_in_title("https://www.example.org/help/page").as_deref(),
            Some("example.org")
        );
        assert_eq!(site_in_title("report.docx - Word"), None);
        assert_eq!(site_in_title("Inbox (3) - Mail"), None);
        assert_eq!(site_in_title("Version 1.2"), None);
        assert_eq!(app_name("excel"), "Excel");
    }
}
