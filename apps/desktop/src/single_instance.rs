//! A process-lifetime Windows ownership handle prevents startup races.
#[cfg(windows)]
pub(super) struct Owner(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(windows)]
pub(super) fn enter(request: &crate::launch::Request) -> Result<Option<Owner>, String> {
    enter_named("Local\\ClipDesigner.Editor.Instance", || {
        crate::tray::forward(request)
    })
}
#[cfg(windows)]
fn enter_named(name: &str, forward: impl Fn() -> bool) -> Result<Option<Owner>, String> {
    use windows_sys::Win32::{Foundation::*, System::Threading::*};
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        return Err("Could not reserve the ClipDesigner application instance.".into());
    }
    let owner = Owner(handle);
    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return Ok(Some(owner));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if matches!(
            unsafe { WaitForSingleObject(handle, 0) },
            WAIT_ABANDONED | WAIT_OBJECT_0
        ) {
            return Ok(Some(owner));
        }
        if forward() {
            return Ok(None);
        }
        if std::time::Instant::now() >= deadline {
            return Err("ClipDesigner is already running but is not responding. Return to its window and try opening the file again.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn simultaneous_startups_forward_then_new_owner_can_start_after_exit() {
        let name = format!(
            "Local\\ClipDesigner.Test.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let owner = enter_named(&name, || panic!("first launch must own"))
            .unwrap()
            .unwrap();
        let other = name.clone();
        std::thread::spawn(move || {
            assert!(enter_named(&other, || true).unwrap().is_none());
        })
        .join()
        .unwrap();
        drop(owner);
        std::thread::spawn(move || {
            assert!(
                enter_named(&name, || panic!("closed owner cannot receive"))
                    .unwrap()
                    .is_some()
            );
        })
        .join()
        .unwrap();
    }
}
