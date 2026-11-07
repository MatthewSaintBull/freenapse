//! "Run at login" entry for the tray app (HKCU\...\Run, no console windows).
//!
//! The entry names the executable's path, so it breaks when freenapse.exe is
//! moved or replaced by a download with another name: Windows skips a missing
//! file at login and Task Manager stops listing it. The user's choice is kept
//! in `HKCU\Software\freenapse` (`StartAtLogin`) as well, and the running tray
//! app points the entry at itself again ([`repair`]), and puts it back if
//! another program deleted it.

#![allow(unsafe_op_in_unsafe_fn)]

use std::ptr;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::*;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE: &str = "freenapse";
const APP_KEY: &str = "Software\\freenapse";
const WANTED: &str = "StartAtLogin";

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

/// The executable a Run command starts: the quoted path, or the text up to the first space.
fn command_exe(cmd: &str) -> &str {
    let cmd = cmd.trim();
    match cmd.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest),
        None => cmd.split(' ').next().unwrap_or(cmd),
    }
}

/// The command this executable registers.
fn own_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" --tray", exe.display()))
}

/// Registered, and the file it starts still exists (Windows skips it otherwise).
pub fn is_enabled() -> bool {
    command().is_some_and(|c| std::path::Path::new(command_exe(&c)).exists())
}

/// The user's choice from the toggle or `freenapse autostart`; None before either was used.
pub fn wanted() -> Option<bool> {
    unsafe {
        let mut key: HKEY = 0;
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(APP_KEY).as_ptr(), 0, KEY_QUERY_VALUE, &mut key) != ERROR_SUCCESS {
            return None;
        }
        let mut v = 0u32;
        let mut len = 4u32;
        let mut kind = 0u32;
        let rc = RegQueryValueExW(key, wide(WANTED).as_ptr(), ptr::null_mut(), &mut kind, &mut v as *mut u32 as *mut u8, &mut len);
        RegCloseKey(key);
        (rc == ERROR_SUCCESS && kind == REG_DWORD).then_some(v != 0)
    }
}

fn set_wanted(on: bool) -> Result<(), String> {
    unsafe {
        let mut key: HKEY = 0;
        let rc = RegCreateKeyExW(HKEY_CURRENT_USER, wide(APP_KEY).as_ptr(), 0, ptr::null(), 0, KEY_SET_VALUE, ptr::null(), &mut key, ptr::null_mut());
        if rc != ERROR_SUCCESS {
            return Err(format!("cannot open HKCU\\{APP_KEY} (error {rc})"));
        }
        let v: u32 = on as u32;
        let rc = RegSetValueExW(key, wide(WANTED).as_ptr(), 0, REG_DWORD, &v as *const u32 as *const u8, 4);
        RegCloseKey(key);
        if rc != ERROR_SUCCESS {
            return Err(format!("registry write failed (error {rc})"));
        }
    }
    Ok(())
}

/// Called by the running tray app. When the user wants it to start at login,
/// the Run entry must start this executable: write it again when it is gone,
/// names a file that no longer exists, or names another copy (the user runs
/// this one now). A Run entry from before the choice was stored counts as the
/// choice. True when the entry was written.
pub fn repair() -> bool {
    let current = command();
    let wanted = match wanted() {
        Some(w) => w,
        None if current.is_some() => {
            let _ = set_wanted(true);
            true
        }
        None => false,
    };
    if !wanted || current.is_some() && current == own_command() {
        return false;
    }
    write_run(true).is_ok()
}

/// Register (or remove) this executable in tray mode (`freenapse.exe --tray`),
/// and remember the choice for [`repair`].
pub fn set(on: bool) -> Result<(), String> {
    write_run(on)?;
    set_wanted(on)
}

fn write_run(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    unsafe {
        let key = open_run_key(KEY_SET_VALUE)?;
        let rc = if on {
            let data = wide(&format!("\"{}\" --tray", exe.display()));
            debug_assert_eq!(Some(format!("\"{}\" --tray", exe.display())), own_command());
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

#[cfg(test)]
mod tests {
    use super::command_exe;

    #[test]
    fn exe_of_a_run_command() {
        assert_eq!(command_exe("\"C:\\Program Files (x86)\\freenapse.exe\" --tray"), "C:\\Program Files (x86)\\freenapse.exe");
        assert_eq!(command_exe("C:\\tools\\freenapse.exe --tray"), "C:\\tools\\freenapse.exe");
        assert_eq!(command_exe("  \"C:\\a b\\freenapse (1).exe\"  "), "C:\\a b\\freenapse (1).exe");
    }
}
