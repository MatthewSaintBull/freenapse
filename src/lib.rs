//! freenapse: a tiny replacement for Razer Synapse for the Razer Viper V3 Pro.
//!
//! Layers:
//! - [`protocol`]: Razer 90-byte report framing and command builders (pure, tested).
//! - [`hid`]: Windows HID transport (SetupAPI + HidD feature reports) with retry logic.
//! - [`device`]: typed API for the mouse (DPI, stages, polling, power, sensor, buttons).
//! - [`sensor`]: Focus Pro tracking/lift-off parameters as Synapse computes them.
//! - [`config`]: TOML configuration file.
//! - [`apply`]: push a configuration to the mouse, writing only what differs.

pub mod accel;
pub mod apply;
pub mod autostart;
pub mod config;
pub mod device;
pub mod diag;
pub mod hid;
pub mod library;
pub mod macros;
pub mod player;
pub mod probe;
pub mod protocol;
pub mod sensor;
pub mod tray;
pub mod ui;
pub mod update;

use std::path::PathBuf;

/// The version from Cargo.toml, as printed by `freenapse version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `config.toml` next to the executable when one exists there (portable use),
/// otherwise `%APPDATA%\freenapse\config.toml`.
pub fn config_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let portable = exe.with_file_name("config.toml");
        if portable.exists() {
            return portable;
        }
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("freenapse").join("config.toml")
}

/// Milliseconds since the last keyboard/mouse input, as reported by Windows.
pub fn idle_ms() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    unsafe {
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if GetLastInputInfo(&mut info) == 0 {
            return 0;
        }
        GetTickCount().wrapping_sub(info.dwTime) as u64
    }
}

/// The wireless mouse only answers while it is awake, i.e. while the user is moving it.
pub fn user_active() -> bool {
    idle_ms() < 1500
}
