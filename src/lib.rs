//! freenapse: a tiny replacement for Razer Synapse for Razer mice. What it
//! knows about each model is one row of the device table in [`device`].
//!
//! Layers:
//! - [`protocol`]: Razer 90-byte report framing and command builders (pure, tested).
//! - [`hid`]: Windows HID transport (SetupAPI + HidD feature reports) with retry logic.
//! - [`device`]: typed API for the mouse (DPI, stages, polling, power, sensor, buttons).
//! - [`sensor`]: Focus Pro tracking/lift-off parameters as Synapse computes them.
//! - [`config`]: TOML configuration file.
//! - [`apply`]: push a configuration to the mouse, writing only what differs.

// Nested `if let`s are kept where they read better than a let-chain.
#![allow(clippy::collapsible_if, clippy::collapsible_match, clippy::items_after_test_module)]

pub mod accel;
pub mod apply;
pub mod autostart;
pub mod config;
pub mod device;
pub mod diag;
pub mod hid;
pub mod library;
pub mod lighting;
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

/// Which config file the mouse opened last uses (see [`config_path`]).
static MOUSE_CONFIG: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);

/// Called when a mouse is opened: `None` for `config.toml`, `Some(name)` for
/// `config-<name>.toml` (from the device table).
pub fn set_mouse_config(name: Option<&'static str>) {
    if let Ok(mut c) = MOUSE_CONFIG.lock() {
        *c = name;
    }
}

/// The file name of a mouse's config: `config.toml` for every mouse whose
/// row of the device table names no file of its own, `config-<name>.toml`
/// otherwise, so two mice on one PC never share settings.
pub fn config_file_name(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("config-{n}.toml"),
        None => "config.toml".to_string(),
    }
}

/// The config file of the mouse opened last (`config.toml` before any is):
/// next to the executable when a `config.toml` exists there (portable use),
/// otherwise in `%APPDATA%\freenapse`.
pub fn config_path() -> PathBuf {
    let file = config_file_name(MOUSE_CONFIG.lock().ok().and_then(|c| *c));
    if let Ok(exe) = std::env::current_exe() {
        if exe.with_file_name("config.toml").exists() {
            return exe.with_file_name(file);
        }
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("freenapse").join(file)
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
