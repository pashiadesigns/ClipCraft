//! egui/winit sets ICON_SMALL only. Set both Windows icon slots from the
//! multi-resolution resources and give each window its own taskbar identity.
use std::ptr::null;
use windows::{
    Win32::{
        Foundation::HWND as Window,
        Storage::EnhancedStorage::{
            PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
            PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
        },
        System::Com::StructuredStorage::PROPVARIANT,
        UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
    },
    core::w,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

fn identity(controller: bool) -> (u16, &'static str) {
    if controller {
        (102, "PashiaDesigns.ClipCraft.CaptureController")
    } else {
        (101, "PashiaDesigns.ClipCraft.Editor")
    }
}
unsafe fn apply(hwnd: HWND, controller: bool) -> windows::core::Result<()> {
    let (resource, app_id) = identity(controller);
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let marker = ((dpi as usize) << 16) | usize::from(resource);
    let branded =
        unsafe { GetPropW(hwnd, w!("ClipCraft.WindowBrand").as_ptr()) } as usize == marker;
    let module = unsafe { GetModuleHandleW(null()) };
    for (slot, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
        let size = unsafe { GetSystemMetricsForDpi(metric, dpi) }.max(16);
        let icon = unsafe {
            LoadImageW(
                module,
                resource as usize as *const u16,
                IMAGE_ICON,
                size,
                size,
                LR_SHARED,
            )
        };
        if icon.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        // LR_SHARED owns these cached resource handles. Never destroy a handle
        // returned by WM_SETICON: the previous icon may be owned by winit.
        // The UI framework can replace an icon after the first native setup.
        // A cached identity must not prevent repairing either icon slot.
        if unsafe { SendMessageW(hwnd, WM_GETICON, slot as usize, 0) } != icon as isize {
            unsafe {
                SendMessageW(hwnd, WM_SETICON, slot as usize, icon as isize);
            }
        }
    }
    if branded {
        return Ok(());
    }
    let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(Window(hwnd))? };
    let executable = std::env::current_exe().map_err(|_| windows::core::Error::from_thread())?;
    let executable = executable.to_string_lossy();
    let (argument, name) = if controller {
        ("--capture-controller", "ClipCraft Capture Controller")
    } else {
        ("--editor", "ClipCraft Editor")
    };
    unsafe {
        // Set the group icon and relaunch information BEFORE the AppID: setting
        // the ID asks Explorer to refresh. Without this resource, a fresh group
        // can fall back to the executable's first (editor) icon, even though its
        // WM_GETICON slots are already correct. Also supports portable launches.
        store.SetValue(
            &PKEY_AppUserModel_RelaunchIconResource,
            &PROPVARIANT::from(format!("{executable},-{resource}").as_str()),
        )?;
        store.SetValue(
            &PKEY_AppUserModel_RelaunchCommand,
            &PROPVARIANT::from(format!("\"{executable}\" {argument}").as_str()),
        )?;
        store.SetValue(
            &PKEY_AppUserModel_RelaunchDisplayNameResource,
            &PROPVARIANT::from(name),
        )?;
        store.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(app_id))?;
        store.Commit()?;
    }
    unsafe {
        SetPropW(hwnd, w!("ClipCraft.WindowBrand").as_ptr(), marker as HANDLE);
    }
    Ok(())
}
struct WindowScan {
    root: isize,
    controller_visible: Option<bool>,
}
unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
    let scan = unsafe { &mut *(data as *mut WindowScan) };
    let mut title = [0u16; 128];
    let length = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) } as usize;
    let controller = title[..length] == "ClipCraft Capture".encode_utf16().collect::<Vec<_>>();
    if controller || hwnd as isize == scan.root {
        let _ = unsafe { apply(hwnd, controller) };
    }
    if controller {
        scan.controller_visible = Some(unsafe { IsWindowVisible(hwnd) } != 0);
    }
    1
}
pub(crate) fn sync_window_icons(root: Option<isize>) {
    let _ = prepare_controller_window(root);
}
pub(crate) fn prepare_controller_window(root: Option<isize>) -> Option<bool> {
    // Only windows owned by this application's current UI thread are inspected.
    let mut scan = WindowScan {
        root: root.unwrap_or(0),
        controller_visible: None,
    };
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(visit),
            &mut scan as *mut WindowScan as isize,
        );
    }
    scan.controller_visible
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_first_launch_windows_have_distinct_large_icons_and_shell_ids() {
        // Owned, never-shown windows only: no desktop takeover, input or capture.
        std::thread::spawn(|| unsafe {
            use windows::Win32::System::Com::{
                COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
            };
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let create = |title: windows::core::PCWSTR| {
                CreateWindowExW(
                    0,
                    w!("STATIC").as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    100,
                    100,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                )
            };
            let editor = create(w!("ClipCraft"));
            let controller = create(w!("ClipCraft Capture"));
            assert!(!editor.is_null() && !controller.is_null());
            sync_window_icons(Some(editor as isize));
            let mut icons = Vec::new();
            for (hwnd, is_controller) in [(editor, false), (controller, true)] {
                assert_eq!(IsWindowVisible(hwnd), 0);
                let big = SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0);
                assert_ne!(big, 0);
                assert_ne!(SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0), 0);
                icons.push(big);
                let store: IPropertyStore = SHGetPropertyStoreForWindow(Window(hwnd)).unwrap();
                assert_eq!(
                    store.GetValue(&PKEY_AppUserModel_ID).unwrap().to_string(),
                    identity(is_controller).1
                );
                let executable = std::env::current_exe().unwrap();
                assert_eq!(
                    store
                        .GetValue(&PKEY_AppUserModel_RelaunchIconResource)
                        .unwrap()
                        .to_string(),
                    format!("{},-{}", executable.display(), identity(is_controller).0)
                );
                let (argument, name) = if is_controller {
                    ("--capture-controller", "ClipCraft Capture Controller")
                } else {
                    ("--editor", "ClipCraft Editor")
                };
                assert_eq!(
                    store
                        .GetValue(&PKEY_AppUserModel_RelaunchCommand)
                        .unwrap()
                        .to_string(),
                    format!("\"{}\" {argument}", executable.display())
                );
                assert_eq!(
                    store
                        .GetValue(&PKEY_AppUserModel_RelaunchDisplayNameResource)
                        .unwrap()
                        .to_string(),
                    name
                );
            }
            assert_ne!(icons[0], icons[1]);
            // Reproduce a framework update after branding; the old marker-only
            // fast path left the incorrect editor icon in place permanently.
            for slot in [ICON_SMALL, ICON_BIG] {
                SendMessageW(controller, WM_SETICON, slot as usize, icons[0]);
            }
            sync_window_icons(Some(editor as isize));
            assert_eq!(
                SendMessageW(controller, WM_GETICON, ICON_BIG as usize, 0),
                icons[1]
            );
            assert_ne!(
                SendMessageW(controller, WM_GETICON, ICON_SMALL as usize, 0),
                icons[0]
            );
            assert_eq!(IsWindowVisible(controller), 0);
            DestroyWindow(controller);
            let reopened = create(w!("ClipCraft Capture"));
            sync_window_icons(Some(editor as isize));
            assert_eq!(
                SendMessageW(reopened, WM_GETICON, ICON_BIG as usize, 0),
                icons[1]
            );
            DestroyWindow(reopened);
            DestroyWindow(editor);
            CoUninitialize();
        })
        .join()
        .unwrap();
    }
    #[test]
    fn controller_and_editor_have_separate_shell_identities_and_resource_ids() {
        assert_eq!(identity(true).0, 102);
        assert_eq!(identity(false).0, 101);
        assert_ne!(identity(true).1, identity(false).1);
    }
}
