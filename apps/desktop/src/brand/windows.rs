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
    System::LibraryLoader::GetModuleHandleW,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

unsafe fn apply(hwnd: HWND) -> windows::core::Result<()> {
    let (resource, app_id) = (101u16, "PashiaDesigns.ClipDesigner");
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let marker = ((dpi as usize) << 16) | usize::from(resource);
    let branded =
        unsafe { GetPropW(hwnd, w!("ClipDesigner.WindowBrand").as_ptr()) } as usize == marker;
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
    let (argument, name) = ("", "ClipDesigner");
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
        SetPropW(
            hwnd,
            w!("ClipDesigner.WindowBrand").as_ptr(),
            marker as HANDLE,
        );
    }
    Ok(())
}
pub(crate) fn sync_window_icons(root: Option<isize>) {
    if let Some(root) = root {
        let _ = unsafe { apply(root as HWND) };
    }
}
