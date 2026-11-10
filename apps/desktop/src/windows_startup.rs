//! Preferences > Start with Windows: the per-user Run entry.
const VALUE: &str = "ClipDesigner";

#[cfg(all(windows, not(test)))]
mod platform {
    use super::VALUE;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    pub(super) fn enabled() -> bool {
        let mut size = 0u32;
        // SAFETY: the strings are null-terminated and no buffer is written;
        // this call only reports whether the value exists and its size.
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                wide(RUN).as_ptr(),
                wide(VALUE).as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            ) == ERROR_SUCCESS
        }
    }
    pub(super) fn set(on: bool) -> Result<(), String> {
        if on == enabled() {
            return Ok(());
        }
        let (key, name) = (wide(RUN), wide(VALUE));
        // SAFETY: every pointer refers to a live, null-terminated buffer, and
        // the size passed is that buffer's length in bytes.
        let status = unsafe {
            if on {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let command = wide(&format!("\"{}\"", exe.display()));
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    command.as_ptr().cast(),
                    (command.len() * 2) as u32,
                )
            } else {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr())
            }
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "Could not change Start with Windows (error {status})."
            ))
        }
    }
}
/// Tests and other platforms never touch the real sign-in list.
#[cfg(not(all(windows, not(test))))]
mod platform {
    use std::sync::atomic::{AtomicBool, Ordering};
    static ON: AtomicBool = AtomicBool::new(false);
    pub(super) fn enabled() -> bool {
        let _ = super::VALUE;
        ON.load(Ordering::Relaxed)
    }
    pub(super) fn set(on: bool) -> Result<(), String> {
        ON.store(on, Ordering::Relaxed);
        Ok(())
    }
}
pub(crate) fn enabled() -> bool {
    platform::enabled()
}
pub(crate) fn set(on: bool) -> Result<(), String> {
    platform::set(on)
}
