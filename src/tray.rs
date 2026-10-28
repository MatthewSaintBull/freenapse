//! Tray application: battery icon, DPI stage / polling rate switching,
//! per-game profiles, software macros, low-battery notification. Pure Win32.
//! Started by `freenapse.exe` when it is launched without arguments.

#![allow(non_snake_case, unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::time::Instant;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::Config;
use crate::device::{Battery, DpiStages, Mouse, RATES_HYPERPOLLING};
use crate::{apply, config_path, ui, user_active};

const WM_TRAY: u32 = WM_APP + 1;
/// Posted by a second instance (or `freenapse ui`) to open the settings window.
const WM_OPEN_SETTINGS: u32 = WM_APP + 5;
const CLASS_NAME: &str = "freenapse-tray";
const TIMER_POLL: usize = 1;
const TIMER_GAME: usize = 2;
const TIMER_RETRY: usize = 3;

const ID_QUIT: usize = 1;
const ID_APPLY: usize = 2;
const ID_OPEN_CONFIG: usize = 3;
const ID_REFRESH: usize = 4;
const ID_SETTINGS: usize = 5;
const ID_STAGE_BASE: usize = 100;
const ID_POLL_BASE: usize = 200;

struct App {
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
    hicon: HICON,
    config: Config,
    config_error: Option<String>,
    mouse: Option<Mouse>,
    battery: Option<Battery>,
    stages: Option<DpiStages>,
    polling: Option<u16>,
    config_applied: bool,
    /// No config file yet: build one from the mouse's current settings once it answers.
    need_import: bool,
    /// Game whose profile is currently written to the mouse (its `GameOverrides::key`), if any.
    game_mode: Option<String>,
    /// Game profile that should be on the mouse but could not be written yet (mouse asleep).
    game_wanted: Option<Option<String>>,
    notified_low: bool,
    last_error: Option<String>,
    last_refresh: Option<Instant>,
    taskbar_created: u32,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

/// Run the tray application on this thread until it quits. A second instance
/// just asks the running one to open the settings window.
pub fn run(open_settings: bool) {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let class_name = wide(CLASS_NAME);
        let mutex = CreateMutexW(ptr::null(), 1, wide("Local\\freenapse-tray").as_ptr());
        if mutex == 0 || GetLastError() == ERROR_ALREADY_EXISTS {
            // Already running: launching again opens the settings window, like Synapse.
            let existing = FindWindowExW(HWND_MESSAGE, 0, class_name.as_ptr(), ptr::null());
            if existing != 0 {
                PostMessageW(existing, WM_OPEN_SETTINGS, 0, 0);
            }
            return;
        }

        let hinstance = GetModuleHandleW(ptr::null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: 0,
            hCursor: 0,
            hbrBackground: 0,
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);

        let need_import = !config_path().exists();
        let (config, config_error) = if need_import {
            (Config::default(), None)
        } else {
            match Config::load(&config_path()) {
                Ok(c) => (c, None),
                Err(e) => (Config::default(), Some(e)),
            }
        };

        load_soft_macros(&config);
        install_hook();
        let app = Box::new(App {
            hwnd: 0,
            nid: mem::zeroed(),
            hicon: 0,
            config,
            config_error,
            mouse: None,
            battery: None,
            stages: None,
            polling: None,
            config_applied: false,
            need_import,
            game_mode: None,
            game_wanted: None,
            notified_low: false,
            last_error: None,
            last_refresh: None,
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        });
        let app_ptr = Box::into_raw(app);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("freenapse").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            0,
            hinstance,
            app_ptr as *const c_void,
        );
        if hwnd == 0 {
            return;
        }
        if open_settings {
            PostMessageW(hwnd, WM_OPEN_SETTINGS, 0, 0);
        }

        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn app_from(hwnd: HWND) -> *mut App {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam as *const CREATESTRUCTW;
            let app = (*cs).lpCreateParams as *mut App;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize);
            (*app).hwnd = hwnd;
            (*app).add_icon();
            (*app).refresh();
            let poll_ms = (*app).config.tray.poll_seconds.max(5) * 1000;
            SetTimer(hwnd, TIMER_POLL, poll_ms, None);
            SetTimer(hwnd, TIMER_GAME, 2000, None);
            SetTimer(hwnd, TIMER_RETRY, 5000, None);
            0
        }
        WM_TIMER => {
            let app = app_from(hwnd);
            if app.is_null() {
                return 0;
            }
            match wparam {
                TIMER_POLL => (*app).refresh(),
                TIMER_GAME => (*app).check_game(),
                TIMER_RETRY => {
                    // Settings could not be written while the mouse was asleep: try again once it moves.
                    if ((*app).need_import || !(*app).config_applied || (*app).battery.is_none()) && user_active() {
                        (*app).refresh();
                    }
                }
                _ => {}
            }
            0
        }
        WM_TRAY => {
            let app = app_from(hwnd);
            let event = (lparam & 0xffff) as u32;
            if event == WM_CONTEXTMENU || event == WM_RBUTTONUP {
                (*app).show_menu();
            } else if event == NIN_SELECT || event == WM_LBUTTONDBLCLK {
                // Left click on the icon opens the settings window, like Synapse.
                ui::open(hwnd);
            }
            0
        }
        WM_DESTROY => {
            let app = app_from(hwnd);
            if !app.is_null() {
                Shell_NotifyIconW(NIM_DELETE, &(*app).nid);
                if (*app).hicon != 0 {
                    DestroyIcon((*app).hicon);
                }
                drop(Box::from_raw(app));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            0
        }
        _ => {
            let app = app_from(hwnd);
            if app.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            match msg {
                m if m == (*app).taskbar_created => {
                    // Explorer restarted: put the icon back.
                    Shell_NotifyIconW(NIM_ADD, &(*app).nid);
                    Shell_NotifyIconW(NIM_SETVERSION, &(*app).nid);
                    0
                }
                WM_OPEN_SETTINGS => {
                    ui::open(hwnd);
                    0
                }
                ui::WM_UI_CONFIG_CHANGED => {
                    // The settings window saved the config file (and wrote the mouse when wparam == 1).
                    match Config::load(&config_path()) {
                        Ok(c) => {
                            (*app).config = c;
                            (*app).config_error = None;
                        }
                        Err(e) => (*app).config_error = Some(e),
                    }
                    load_soft_macros(&(*app).config);
                    (*app).config_applied = wparam == 1;
                    // The config (and the mouse) now hold the base layout; a running game is re-detected.
                    (*app).game_mode = None;
                    (*app).game_wanted = None;
                    (*app).refresh();
                    (*app).check_game();
                    0
                }
                ui::WM_UI_CLOSED => 0,
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }
}

impl App {
    unsafe fn add_icon(&mut self) {
        self.nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
        self.nid.hWnd = self.hwnd;
        self.nid.uID = 1;
        self.nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        self.nid.uCallbackMessage = WM_TRAY;
        self.nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        self.set_icon();
        copy_wide(&mut self.nid.szTip, "freenapse: connecting...");
        Shell_NotifyIconW(NIM_ADD, &self.nid);
        Shell_NotifyIconW(NIM_SETVERSION, &self.nid);
    }

    unsafe fn set_icon(&mut self) {
        let new_icon = make_battery_icon(self.battery, self.mouse.is_some());
        if new_icon != 0 {
            if self.hicon != 0 {
                DestroyIcon(self.hicon);
            }
            self.hicon = new_icon;
            self.nid.hIcon = self.hicon;
        }
    }

    unsafe fn update_tray(&mut self) {
        self.set_icon();
        let mut tip = String::from("freenapse");
        match (self.battery, self.mouse.as_ref()) {
            (Some(b), _) => {
                tip.push_str(&format!("\nBattery {}%{}", b.percent, if b.charging { " (charging)" } else { "" }));
            }
            (None, Some(_)) => tip.push_str("\nMouse asleep: move it to refresh"),
            (None, None) => tip.push_str("\nMouse not found"),
        }
        if let Some(st) = &self.stages {
            if let Some(d) = st.stages.get(st.active.saturating_sub(1) as usize) {
                tip.push_str(&format!("\nDPI {}", d.0));
            }
        }
        if let Some(p) = self.polling {
            tip.push_str(&format!("\n{p} Hz"));
        }
        if let Some(g) = self.game_mode.as_deref().and_then(|k| self.config.game_by_key(k)) {
            tip.push_str(&format!("\nGame profile: {}", g.name));
        }
        if let Some(e) = &self.config_error {
            tip.push_str(&format!("\nConfig error: {e}"));
        }
        copy_wide(&mut self.nid.szTip, &tip);
        self.nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        Shell_NotifyIconW(NIM_MODIFY, &self.nid);
    }

    unsafe fn notify(&mut self, title: &str, text: &str, warning: bool) {
        copy_wide(&mut self.nid.szInfoTitle, title);
        copy_wide(&mut self.nid.szInfo, text);
        self.nid.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO };
        self.nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP | NIF_INFO;
        Shell_NotifyIconW(NIM_MODIFY, &self.nid);
        self.nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
    }

    /// Polling rates the menu offers: the open mouse's, else the full HyperPolling list.
    fn rates(&self) -> &'static [u16] {
        self.mouse.as_ref().map(|m| m.spec().polling_rates).unwrap_or(&RATES_HYPERPOLLING)
    }

    fn wanted_polling(&self) -> u16 {
        self.game_mode.as_deref().and_then(|g| self.config.game_by_key(g)).map(|g| g.polling).unwrap_or(self.config.polling.rate)
    }

    /// Reconnect if needed, apply the config once, read battery/DPI/polling.
    unsafe fn refresh(&mut self) {
        if self.mouse.is_none() {
            match Mouse::open() {
                Ok(m) => self.mouse = Some(m),
                Err(e) => {
                    self.last_error = Some(e.to_string());
                    self.battery = None;
                    self.update_tray();
                    return;
                }
            }
        }
        struct Read {
            applied: Option<usize>,
            battery: Battery,
            stages: Option<DpiStages>,
            polling: Option<u16>,
        }
        if self.need_import {
            let imported = apply::import(self.mouse.as_ref().unwrap());
            match imported {
                Ok(cfg) => {
                    if let Err(e) = Config::write_new(&config_path(), &cfg) {
                        self.config_error = Some(e);
                    }
                    load_soft_macros(&cfg);
                    self.config = cfg;
                    self.need_import = false;
                    self.config_applied = true;
                    self.notify("freenapse", "Saved the mouse's current settings as your config file", false);
                }
                Err(e) => {
                    self.last_error = Some(e.to_string());
                    self.update_tray();
                    return;
                }
            }
        }
        let need_apply = !self.config_applied && self.config_error.is_none();
        let wanted = self.wanted_polling();
        let result = {
            let mouse = self.mouse.as_ref().unwrap();
            (|| -> Result<Read, crate::hid::HidError> {
                let applied = if need_apply { Some(apply::apply(mouse, &self.config, wanted)?.len()) } else { None };
                Ok(Read { applied, battery: mouse.battery()?, stages: mouse.dpi_stages().ok(), polling: mouse.polling_rate().ok() })
            })()
        };
        match result {
            Ok(r) => {
                if let Some(n) = r.applied {
                    self.config_applied = true;
                    // apply() wrote the base layout: a running game's profile is re-applied by check_game.
                    self.game_mode = None;
                    self.game_wanted = None;
                    if n > 0 {
                        self.notify("freenapse", &format!("Applied {n} setting(s) to the mouse"), false);
                    }
                }
                self.battery = Some(r.battery);
                self.stages = r.stages;
                self.polling = r.polling;
                self.last_refresh = Some(Instant::now());
                self.last_error = None;
                let low = self.config.tray.notify_percent;
                let b = r.battery;
                if !b.charging && b.percent <= low && !self.notified_low {
                    self.notified_low = true;
                    let name = self.mouse.as_ref().map(|m| m.spec().name.to_string()).unwrap_or_else(|| "Mouse".into());
                    self.notify("Mouse battery low", &format!("{name} at {}%. Plug it in.", b.percent), true);
                }
                if b.charging || b.percent > low + 5 {
                    self.notified_low = false;
                }
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                match e {
                    // Device unplugged: drop the handle so the next refresh re-enumerates.
                    crate::hid::HidError::Win32(..) | crate::hid::HidError::NotFound => {
                        self.mouse = None;
                        self.battery = None;
                    }
                    // Asleep: keep the last known battery value.
                    _ => {}
                }
            }
        }
        self.update_tray();
    }

    /// Per-game profiles: when a configured game owns the foreground window,
    /// write its polling rate, DPI stage and button layout; restore the base
    /// config when it goes away. Like Synapse's in-game polling rate and
    /// per-game profiles, this is done by the tray app.
    unsafe fn check_game(&mut self) {
        if self.config.games.is_empty() && self.config.polling.games.is_empty() {
            return;
        }
        let path = foreground_path().unwrap_or_default();
        let game = self.config.game_for(&path).map(|g| g.key);
        // Keep the mouse's state when nothing changed and nothing is left to write.
        let wanted = match &self.game_wanted {
            Some(w) if *w == game => Some(game.clone()),
            _ if game == self.game_mode => None,
            _ => Some(game.clone()),
        };
        let Some(wanted) = wanted else { return };
        if !user_active() {
            // The mouse is probably asleep: remember and write once it moves.
            self.game_wanted = Some(wanted);
            return;
        }
        let Some(mouse) = self.mouse.as_ref() else {
            self.game_wanted = Some(wanted);
            return;
        };
        let cfg = &self.config;
        let over = wanted.as_deref().and_then(|g| cfg.game_by_key(g));
        let result: Result<(), crate::hid::HidError> = (|| {
            let polling = over.as_ref().map(|g| g.polling).unwrap_or(cfg.polling.rate);
            if mouse.polling_rate().ok() != Some(polling) {
                mouse.set_polling_rate(polling)?;
            }
            let stage = over.as_ref().and_then(|g| g.dpi_stage).unwrap_or(cfg.dpi.active);
            let st = mouse.dpi_stages()?;
            if st.active != stage {
                mouse.set_dpi_stages(stage, &st.stages)?;
            }
            apply::apply_game_buttons(mouse, cfg, over.as_ref())?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.game_mode = wanted;
                self.game_wanted = None;
                self.polling = mouse.polling_rate().ok();
                self.stages = mouse.dpi_stages().ok();
                self.update_tray();
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                self.game_wanted = Some(wanted);
            }
        }
    }

    unsafe fn show_menu(&mut self) {
        let menu = CreatePopupMenu();
        let status = match (self.battery, self.mouse.as_ref()) {
            (Some(b), _) => format!("Battery {}%{}", b.percent, if b.charging { " (charging)" } else { "" }),
            (None, Some(_)) => "Mouse asleep - move it".to_string(),
            (None, None) => "Mouse not found".to_string(),
        };
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, wide(&status).as_ptr());
        if let Some(e) = &self.last_error {
            if self.battery.is_none() {
                AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, wide(e).as_ptr());
            }
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());

        let stages = self.stages.clone().map(|s| s.stages).unwrap_or_else(|| self.config.stage_pairs());
        let active = self.stages.as_ref().map(|s| s.active).unwrap_or(self.config.dpi.active);
        for (i, (x, _)) in stages.iter().enumerate() {
            let checked = if i + 1 == active as usize { MF_CHECKED } else { 0 };
            AppendMenuW(menu, MF_STRING | checked, ID_STAGE_BASE + i, wide(&format!("DPI {x}")).as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        for (i, hz) in self.rates().iter().enumerate() {
            let checked = if Some(*hz) == self.polling { MF_CHECKED } else { 0 };
            AppendMenuW(menu, MF_STRING | checked, ID_POLL_BASE + i, wide(&format!("{hz} Hz")).as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        AppendMenuW(menu, MF_STRING, ID_SETTINGS, wide("Settings...").as_ptr());
        AppendMenuW(menu, MF_STRING, ID_REFRESH, wide("Refresh").as_ptr());
        AppendMenuW(menu, MF_STRING, ID_APPLY, wide("Re-apply config").as_ptr());
        AppendMenuW(menu, MF_STRING, ID_OPEN_CONFIG, wide("Open config file").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        AppendMenuW(menu, MF_STRING, ID_QUIT, wide("Quit").as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(self.hwnd);
        let id = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, self.hwnd, ptr::null());
        PostMessageW(self.hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        self.handle_command(id as usize);
    }

    unsafe fn handle_command(&mut self, id: usize) {
        match id {
            0 => {}
            ID_QUIT => {
                DestroyWindow(self.hwnd);
            }
            ID_REFRESH => self.refresh(),
            ID_SETTINGS => {
                ui::open(self.hwnd);
            }
            ID_APPLY => {
                match Config::load_or_create(&config_path()) {
                    Ok(c) => {
                        self.config = c;
                        self.config_error = None;
                    }
                    Err(e) => self.config_error = Some(e),
                }
                load_soft_macros(&self.config);
                self.config_applied = false;
                self.refresh();
            }
            ID_OPEN_CONFIG => {
                let path = config_path();
                let _ = Config::load_or_create(&path);
                ShellExecuteW(0, wide("open").as_ptr(), wide(&path.to_string_lossy()).as_ptr(), ptr::null(), ptr::null(), SW_SHOWNORMAL);
            }
            id if (ID_STAGE_BASE..ID_STAGE_BASE + 5).contains(&id) => {
                let idx = id - ID_STAGE_BASE;
                if let (Some(mouse), Some(st)) = (self.mouse.as_ref(), self.stages.as_ref()) {
                    if idx < st.stages.len() {
                        let r = mouse.set_dpi_stages((idx + 1) as u8, &st.stages);
                        if let Err(e) = r {
                            self.last_error = Some(e.to_string());
                        }
                        self.stages = mouse.dpi_stages().ok();
                        self.update_tray();
                    }
                }
            }
            id if (ID_POLL_BASE..ID_POLL_BASE + self.rates().len()).contains(&id) => {
                let hz = self.rates()[id - ID_POLL_BASE];
                if let Some(mouse) = self.mouse.as_ref() {
                    match mouse.set_polling_rate(hz) {
                        Ok(()) => {
                            self.polling = Some(hz);
                        }
                        Err(e) => self.last_error = Some(e.to_string()),
                    }
                    self.update_tray();
                }
            }
            _ => {}
        }
    }
}

/// Full path of the executable owning the foreground window.
unsafe fn foreground_path() -> Option<String> {
    let hwnd = GetForegroundWindow();
    if hwnd == 0 {
        return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 {
        return None;
    }
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if h == 0 {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
    CloseHandle(h);
    if ok == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

// ---------------------------------------------------------------------------
// Software macros. The Viper V3 Pro has no macro memory, so (like Synapse) the
// macro is played from the PC: the bound button sends F13..F24, a low-level
// keyboard hook swallows that key and plays the macro's events.
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::macros::{Event, Playback};
use crate::player;

struct SoftSlot {
    events: Arc<Vec<Event>>,
    play: Playback,
    /// Flag of the playback loop in progress ("while held" / toggle); set to stop it.
    stop: Mutex<Arc<AtomicBool>>,
    running: AtomicBool,
    down: AtomicBool,
    seq: AtomicUsize,
}

static SOFT: Mutex<Vec<Arc<SoftSlot>>> = Mutex::new(Vec::new());

/// Rebuild the slot table from the config (slot i = key F13 + i, same order as `Config::soft_macro_slots`).
fn load_soft_macros(cfg: &Config) {
    let slots: Vec<Arc<SoftSlot>> = cfg
        .soft_macro_slots()
        .into_iter()
        .map(|(id, play)| {
            let events = cfg.macro_by_id(id).and_then(|m| m.events().ok()).unwrap_or_default();
            Arc::new(SoftSlot {
                events: Arc::new(events),
                play,
                stop: Mutex::new(Arc::new(AtomicBool::new(false))),
                running: AtomicBool::new(false),
                down: AtomicBool::new(false),
                seq: AtomicUsize::new(0),
            })
        })
        .collect();
    let mut table = SOFT.lock().unwrap_or_else(|e| e.into_inner());
    for s in table.iter() {
        s.stop.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Relaxed);
    }
    *table = slots;
}

/// The low-level keyboard hook gets a thread of its own: Windows calls it for
/// every key press system-wide and waits for it, so it must never sit behind
/// the tray's device reads (up to a second each while the mouse sleeps).
/// Windows also silently removes a hook that keeps timing out.
unsafe fn install_hook() {
    std::thread::spawn(|| unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_hook), GetModuleHandleW(ptr::null()), 0);
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn kb_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        let own = k.dwExtraInfo == player::INJECT_MARK;
        if !own && (player::VK_F13 as u32..=player::VK_F24 as u32).contains(&k.vkCode) {
            let slot = (k.vkCode - player::VK_F13 as u32) as usize;
            let s = SOFT.lock().ok().and_then(|t| t.get(slot).cloned());
            if let Some(s) = s {
                let down = wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN;
                soft_trigger(&s, down);
                return 1;
            }
        }
    }
    CallNextHookEx(0, code, wparam, lparam)
}

/// Start a fresh stop flag for a playback loop, ending the previous one.
fn new_run(s: &SoftSlot) -> Arc<AtomicBool> {
    let mut g = s.stop.lock().unwrap_or_else(|e| e.into_inner());
    g.store(true, Ordering::Relaxed);
    *g = Arc::new(AtomicBool::new(false));
    g.clone()
}

fn soft_trigger(s: &Arc<SoftSlot>, down: bool) {
    let was_down = s.down.swap(down, Ordering::Relaxed);
    if down && was_down {
        return; // keyboard auto-repeat of the held key
    }
    match s.play {
        Playback::Times(n) => {
            if down {
                let s = s.clone();
                let stop = new_run(&s);
                std::thread::spawn(move || {
                    for _ in 0..n.max(1) {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        player::play(&s.events, &stop);
                    }
                });
            }
        }
        Playback::Held => {
            if down {
                let s = s.clone();
                let stop = new_run(&s);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        player::play(&s.events, &stop);
                    }
                });
            } else {
                s.stop.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Relaxed);
            }
        }
        Playback::Toggle => {
            if !down {
                return;
            }
            if s.running.swap(false, Ordering::Relaxed) {
                s.stop.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Relaxed);
            } else {
                s.running.store(true, Ordering::Relaxed);
                let s = s.clone();
                let stop = new_run(&s);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        player::play(&s.events, &stop);
                    }
                    s.running.store(false, Ordering::Relaxed);
                });
            }
        }
        Playback::Sequence => {
            if down {
                let steps = player::sequence_steps(&s.events);
                if steps.is_empty() {
                    return;
                }
                let k = s.seq.fetch_add(1, Ordering::Relaxed) % steps.len();
                let step = steps[k].clone();
                std::thread::spawn(move || player::play(&step, &AtomicBool::new(false)));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Icon rendering: a battery outline with the percentage drawn in a 3x5 font.
// ---------------------------------------------------------------------------

const FONT_3X5: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

struct Canvas {
    w: i32,
    h: i32,
    px: Vec<[u8; 4]>, // BGRA, premultiplied (alpha is 0 or 255 here)
}

impl Canvas {
    fn new(w: i32, h: i32) -> Canvas {
        Canvas { w, h, px: vec![[0, 0, 0, 0]; (w * h) as usize] }
    }
    fn set(&mut self, x: i32, y: i32, rgb: (u8, u8, u8)) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = [rgb.2, rgb.1, rgb.0, 255];
        }
    }
    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, rgb: (u8, u8, u8)) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.set(x, y, rgb);
            }
        }
    }
    fn digit(&mut self, d: u8, x: i32, y: i32, scale: i32, rgb: (u8, u8, u8)) {
        let glyph = FONT_3X5[(d % 10) as usize];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    self.rect(x + col * scale, y + row as i32 * scale, x + (col + 1) * scale, y + (row as i32 + 1) * scale, rgb);
                }
            }
        }
    }
}

unsafe fn make_battery_icon(battery: Option<Battery>, connected: bool) -> HICON {
    let size = GetSystemMetrics(SM_CXSMICON).max(16);
    let scale = (size / 16).max(1);
    let mut c = Canvas::new(size, size);

    let white = (235, 235, 235);
    let grey = (140, 140, 140);
    let color = match battery {
        Some(b) if b.charging => (80, 160, 255),
        Some(b) if b.percent <= 15 => (235, 60, 60),
        Some(b) if b.percent <= 35 => (240, 180, 40),
        Some(_) => (90, 200, 90),
        None => grey,
    };

    // Battery body: outline 1px, with a nub on the right; occupies rows 2..14 of a 16px grid.
    let x0 = 0;
    let y0 = 2 * scale;
    let x1 = 14 * scale;
    let y1 = 14 * scale;
    let outline = if connected { white } else { grey };
    c.rect(x0, y0, x1, y0 + scale, outline);
    c.rect(x0, y1 - scale, x1, y1, outline);
    c.rect(x0, y0, x0 + scale, y1, outline);
    c.rect(x1 - scale, y0, x1, y1, outline);
    c.rect(x1, y0 + 3 * scale, x1 + 2 * scale, y1 - 3 * scale, outline);

    match battery {
        Some(b) => {
            // Fill level along the bottom edge inside the outline.
            let inner_w = x1 - x0 - 2 * scale;
            let fill = (inner_w * b.percent as i32 / 100).max(if b.percent > 0 { scale } else { 0 });
            c.rect(x0 + scale, y1 - 3 * scale, x0 + scale + fill, y1 - scale, color);
            // Percentage digits, centred in the upper area.
            let digits: Vec<u8> = if b.percent >= 100 { vec![1, 0, 0] } else if b.percent >= 10 { vec![b.percent / 10, b.percent % 10] } else { vec![b.percent] };
            let text_w = (digits.len() as i32 * 4 - 1) * scale;
            let mut x = x0 + (x1 - x0 - text_w) / 2;
            let y = y0 + 2 * scale;
            for d in digits {
                c.digit(d, x, y, scale, white);
                x += 4 * scale;
            }
        }
        None => {
            // A dash when nothing is known.
            c.rect(x0 + 4 * scale, (y0 + y1) / 2, x1 - 4 * scale, (y0 + y1) / 2 + scale, grey);
        }
    }
    canvas_to_icon(&c)
}

unsafe fn canvas_to_icon(c: &Canvas) -> HICON {
    let hdc = GetDC(0);
    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = c.w;
    bmi.bmiHeader.biHeight = -c.h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;
    let mut bits: *mut c_void = ptr::null_mut();
    let color = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, 0, 0);
    ReleaseDC(0, hdc);
    if color == 0 || bits.is_null() {
        return 0;
    }
    ptr::copy_nonoverlapping(c.px.as_ptr() as *const u8, bits as *mut u8, c.px.len() * 4);
    let mask = CreateBitmap(c.w, c.h, 1, 1, ptr::null());
    let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&info);
    DeleteObject(color);
    DeleteObject(mask);
    icon
}
