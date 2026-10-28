//! "Run at login" entry for the tray app (HKCU\...\Run, no console windows).

#![allow(unsafe_op_in_unsafe_fn)]

use std::ptr;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::*;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE: &str = "freenapse";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn open_run_key(access: u32) -> Result<HKEY, String> {
    let mut key: HKEY = 0;
    let rc = RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, access, &mut key);
    if rc != ERROR_SUCCESS {
        return Err(format!("cannot open the Run registry key (error {rc})"));
    }
    Ok(key)
}

/// The command registered to run at login, if any.
pub fn command() -> Option<String> {
    unsafe {
        let key = open_run_key(KEY_QUERY_VALUE).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        let mut kind = 0u32;
        let rc = RegQueryValueExW(key, wide(VALUE).as_ptr(), ptr::null_mut(), &mut kind, buf.as_mut_ptr() as *mut u8, &mut len);
        RegCloseKey(key);
        if rc != ERROR_SUCCESS {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_string())
    }
}

pub fn is_enabled() -> bool {
    command().is_some()
}

/// Register (or remove) this executable in tray mode (`freenapse.exe --tray`).
pub fn set(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    unsafe {
        let key = open_run_key(KEY_SET_VALUE)?;
        let rc = if on {
            let data = wide(&format!("\"{}\" --tray", exe.display()));
            RegSetValueExW(key, wide(VALUE).as_ptr(), 0, REG_SZ, data.as_ptr() as *const u8, (data.len() * 2) as u32)
        } else {
            let rc = RegDeleteValueW(key, wide(VALUE).as_ptr());
            if rc == 2 { ERROR_SUCCESS } else { rc } // ERROR_FILE_NOT_FOUND: already off
        };
        RegCloseKey(key);
        if rc != ERROR_SUCCESS {
            return Err(format!("registry write failed (error {rc})"));
        }
    }
    Ok(())
}
