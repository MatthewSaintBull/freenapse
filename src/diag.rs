//! Diagnostics: the crash report written when freenapse panics, and the HID
//! traffic log (`freenapse --log`). Both live next to the config file, so a
//! portable install keeps them on its own drive.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The folder of the config file: `%APPDATA%\freenapse`, or the executable's
/// folder in portable mode.
pub fn data_dir() -> PathBuf {
    crate::config_path().parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Where a panic is written.
pub fn crash_log_path() -> PathBuf {
    data_dir().join("crash.txt")
}

/// The default HID log path when `--log` is given without one.
pub fn default_hid_log_path() -> PathBuf {
    data_dir().join("hid.log")
}

static HID_LOG: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Turn on the HID traffic log for this process. The `FREENAPSE_HIDLOG`
/// environment variable does the same for a process started elsewhere.
pub fn set_hid_log(path: PathBuf) {
    let _ = HID_LOG.set(Some(path));
}

/// The HID log file, if logging is on.
pub fn hid_log() -> Option<PathBuf> {
    HID_LOG.get_or_init(|| std::env::var_os("FREENAPSE_HIDLOG").map(PathBuf::from)).clone()
}

/// Install the panic hook. The report goes to `crash.txt`; in GUI mode (the
/// tray and the settings window have no console) a message box says where,
/// since with `panic = "abort"` the process is gone right after.
pub fn install_panic_hook(gui: bool) {
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "(no message)".to_string()
        };
        // `freenapse status | head`: the reader went away. Not a crash; leave quietly.
        if message.starts_with("failed printing to std") {
            std::process::exit(0);
        }
        let thread = std::thread::current();
        let args: Vec<String> = std::env::args().skip(1).collect();
        let report = crash_report(&message, &location, thread.name().unwrap_or("?"), &args);
        let path = crash_log_path();
        let written = write_report(&path, &report);
        eprint!("{report}");
        if gui {
            let where_ = if written { format!("Details were written to\n{}", path.display()) } else { "The crash report could not be written.".to_string() };
            let text = format!(
                "freenapse {} stopped because of an internal error:\n\n{message}\n\n{where_}\n\nPlease paste that file into an issue at\nhttps://github.com/MatthewSaintBull/freenapse/issues",
                crate::VERSION
            );
            message_box("freenapse stopped", &text);
        }
    }));
}

/// The text of one crash report.
pub fn crash_report(message: &str, location: &str, thread: &str, args: &[String]) -> String {
    let mut s = String::new();
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let _ = writeln!(s, "freenapse {} crashed at {} UTC", crate::VERSION, format_utc(secs));
    let _ = writeln!(s, "panic:    {message}");
    let _ = writeln!(s, "where:    {location}");
    let _ = writeln!(s, "thread:   {thread}");
    let _ = writeln!(s, "args:     {}", if args.is_empty() { "(none: tray mode)".to_string() } else { args.join(" ") });
    let _ = writeln!(s, "windows:  {}", windows_version());
    let _ = writeln!(s);
    s
}

/// Append the report to the file, creating the folder if needed. True when it got there.
pub fn write_report(path: &std::path::Path, report: &str) -> bool {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::OpenOptions::new().append(true).create(true).open(path).and_then(|mut f| f.write_all(report.as_bytes())).is_ok()
}

/// `2026-10-30 14:05:09` from seconds since the Unix epoch.
pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// The Windows build, from the registry (the version API answers 6.2 to a
/// program without a manifest). Example: `build 26200 (25H2)`.
fn windows_version() -> String {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let key: Vec<u16> = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion".encode_utf16().chain(Some(0)).collect();
    let read = |name: &str| -> String {
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut buf = [0u16; 128];
        let mut size = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _, &mut size)
        };
        if rc != 0 {
            return String::new();
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    };
    let (build, display) = (read("CurrentBuildNumber"), read("DisplayVersion"));
    if build.is_empty() {
        return "unknown".into();
    }
    if display.is_empty() { format!("build {build}") } else { format!("build {build} ({display})") }
}

fn message_box(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW};
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (title, text) = (w(title), w(text));
    unsafe { MessageBoxW(0, text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR | MB_TOPMOST | MB_SETFOREGROUND) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00");
        assert_eq!(format_utc(1_793_318_400), "2026-10-30 00:00:00");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(format_utc(1_709_251_199), "2024-02-29 23:59:59");
    }

    #[test]
    fn report_is_appended() {
        let dir = std::env::temp_dir().join(format!("freenapse-diag-{}", std::process::id()));
        let path = dir.join("sub").join("crash.txt");
        let args = vec!["status".to_string()];
        let report = crash_report("boom", "src/x.rs:1:2", "main", &args);
        assert!(report.starts_with(&format!("freenapse {} crashed at ", crate::VERSION)));
        assert!(report.contains("panic:    boom\n"));
        assert!(report.contains("args:     status\n"));
        assert!(write_report(&path, &report));
        assert!(write_report(&path, &report));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("panic:    boom").count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
