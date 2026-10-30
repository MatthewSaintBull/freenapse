//! Settings window: a small native UI, pure Win32 + GDI.
//!
//! Same user experience as Synapse 4's mouse page: tabs CUSTOMIZE /
//! PERFORMANCE / CALIBRATION / POWER, every change is written to the mouse
//! (and the config file) the moment you make it, no "apply" button. If the
//! wireless mouse is asleep the change is kept pending and written once it
//! wakes up.
//!
//! Everything is drawn by hand (cards, toggles, chips, sliders); the only real
//! child controls are the EDIT boxes for typing numbers and key names.

#![allow(non_snake_case, unsafe_op_in_unsafe_fn)]

mod art;
mod draw;
mod pages;
mod record;

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::mpsc;
use std::time::Instant;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::accel;
use crate::config::{Config, GameConfig, MacroConfig, macro_spec, parse_button};
use crate::device::{BUTTONS, Battery, ButtonFunction, DeviceSpec, MacroStorage, Mouse, PairedDevice};
use crate::hid::HidError;
use crate::library::{self, Game};
use crate::macros::{self, Playback};
use crate::{apply, autostart, config_path, user_active};
use draw::theme::*;
use draw::*;

/// Posted to the owner window after the settings window changed the config file.
/// `wparam` = 1 when the change also reached the mouse, 0 when it is still pending.
pub const WM_UI_CONFIG_CHANGED: u32 = WM_APP + 3;
/// Posted to the owner window when the settings window closes.
pub const WM_UI_CLOSED: u32 = WM_APP + 4;
/// Posted by the game scan thread; `lparam` = `Box<Vec<Game>>`.
const WM_LIBRARY: u32 = WM_APP + 10;
/// Posted by the mouse I/O thread when a job is done; `lparam` = `Box<Done>`.
const WM_IO_DONE: u32 = WM_APP + 11;

/// Result of a library scan: each game with its cover image and program icon, when found.
type Scanned = Vec<(Game, Option<art::Cover>, Option<art::ExeIcon>)>;
/// Decoded cover size in device pixels (tiles are drawn smaller).
const COVER_W: i32 = 300;
const COVER_H: i32 = 450;
/// Program icon size asked from the shell.
const ICON_PX: i32 = 96;

/// Work for the mouse I/O thread. It runs there with the device and returns
/// what to do with the result on the window's thread.
type Job = Box<dyn FnOnce(&mut Dev) -> Done + Send>;
type Done = Box<dyn FnOnce(&mut Ui) + Send>;

/// The device, owned by the window's mouse I/O thread. Every exchange with the
/// dongle takes at least 150 ms (over a second while the mouse sleeps), so none
/// of them may run on the thread that paints and handles clicks.
pub struct Dev {
    mouse: Option<Mouse>,
    /// The dongle did not answer the auto-pairing query once: do not ask again.
    no_auto_pair: bool,
}

impl Dev {
    fn mouse(&mut self) -> Result<&Mouse, HidError> {
        if self.mouse.is_none() {
            self.mouse = Some(Mouse::open()?);
        }
        Ok(self.mouse.as_ref().unwrap())
    }

    /// Drop the handle after an error that means the device went away.
    fn check<T>(&mut self, r: &Result<T, HidError>) {
        if matches!(r, Err(HidError::Win32(..)) | Err(HidError::NotFound)) {
            self.mouse = None;
        }
    }

    fn auto_pairing(&mut self) -> Option<bool> {
        if self.no_auto_pair {
            return None;
        }
        let r = self.mouse.as_ref()?.auto_pairing().ok();
        self.no_auto_pair = r.is_none();
        r
    }
}

fn is_gone(e: &HidError) -> bool {
    matches!(e, HidError::Win32(..) | HidError::NotFound)
}

fn is_asleep_error(e: &HidError) -> bool {
    matches!(e, HidError::Status(crate::protocol::Status::Timeout) | HidError::NoResponse)
}

fn clamp_dpi(v: i32, spec: &DeviceSpec) -> i32 {
    v.clamp(100, spec.dpi_max as i32)
}

const CLASS_NAME: &str = "freenapse-settings";
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_TICK: usize = 1;
const TIMER_BATTERY: usize = 2;
const TIMER_RECORD: usize = 3;
/// Pairing mode is cancelled after this long without a result.
const PAIRING_SECS: u64 = 90;

const WIN_W: i32 = 1240;
const WIN_H: i32 = 800;
const MIN_W: i32 = 980;
const MIN_H: i32 = 620;
/// Navigation pane on the left, page header on top, status line at the bottom (DESIGN.md 3.3).
const SIDEBAR_W: i32 = 240;
const TOPBAR_H: i32 = 88;
const FOOTER_H: i32 = 28;
/// Device card at the bottom of the navigation pane.
const DEVICE_CARD_H: i32 = 88;
/// Margin between the pane and the page content.
const MARGIN: i32 = 24;
/// Settings pages keep their column this wide; only the Games grid uses the full width.
const CONTENT_MAX_W: i32 = 1064;
/// Navigation item height and the pane's inner margin.
const NAV_H: i32 = 36;
const NAV_PAD: i32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Customize,
    Performance,
    Calibration,
    Power,
    Games,
    Roadmap,
    App,
}

impl Page {
    pub const TABS: [Page; 6] = [Page::Customize, Page::Performance, Page::Calibration, Page::Power, Page::Games, Page::Roadmap];

    pub fn title(self) -> &'static str {
        match self {
            Page::Customize => "CUSTOMIZE",
            Page::Performance => "PERFORMANCE",
            Page::Calibration => "CALIBRATION",
            Page::Power => "POWER",
            Page::Games => "GAMES",
            Page::Roadmap => "ROADMAP",
            Page::App => "SETTINGS",
        }
    }

    /// Sidebar and title bar name.
    pub fn label(self) -> &'static str {
        match self {
            Page::Customize => "Customize",
            Page::Performance => "Performance",
            Page::Calibration => "Calibration",
            Page::Power => "Power",
            Page::Games => "Games",
            Page::Roadmap => "Roadmap",
            Page::App => "Settings",
        }
    }

    /// One line under the title.
    pub fn subtitle(self) -> &'static str {
        match self {
            Page::Customize => "Button assignments, Hypershift and macros, stored in the mouse",
            Page::Performance => "Sensitivity, Dynamic Sensitivity, polling rate and rotation",
            Page::Calibration => "Lift-off and landing on your surface",
            Page::Power => "Battery, sleep and the HyperPolling dongle",
            Page::Games => "Profiles that switch on while a game is in the foreground",
            Page::Roadmap => "What freenapse does compared with Razer Synapse 4",
            Page::App => "Tray app, onboard profile, dongle pairing and device info",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Page::Customize => Icon::Mouse,
            Page::Performance => Icon::Speed,
            Page::Calibration => Icon::Track,
            Page::Power => Icon::Battery,
            Page::Games => Icon::Game,
            Page::Roadmap => Icon::Flag,
            Page::App => Icon::Gear,
        }
    }
}

/// What a widget stands for; the value passed with a change depends on the kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Id {
    None,
    Tab(usize),
    Gear,
    StagesEnabled,
    StageCount,
    StageValue(usize),
    StageY(usize),
    XySplit,
    StageActive(usize),
    AccelOn,
    AccelMode,
    Curve,
    CurveAdd,
    CurveRemove,
    CurveTemplate(usize),
    Turbo,
    TurboRate,
    Polling,
    GameRate,
    GamesEdit,
    RotationOn,
    Rotation,
    TrackMode,
    Asym,
    Lift,
    Land,
    Profile,
    Idle,
    LowBattery,
    DongleLed,
    NotifyPercent,
    PollSeconds,
    Autostart,
    OpenConfig,
    Reimport,
    Reapply,
    Refresh,
    SelectButton(usize),
    Layer,
    Category(usize),
    Option(usize),
    KeyEdit,
    /// The text box of the Text function on the Customize page.
    TextEdit,
    MacroPlay,
    MacroTimes,
    SelectMacro(usize),
    MacroNew,
    MacroRecord,
    MacroStop,
    MacroDelete,
    MacroName,
    /// A game profile's row (index into `cfg.games`): opens or closes its settings.
    GameRow(usize),
    /// An installed game without a profile (index into `Ui::library`): creates one.
    GameLib(usize),
    GameFilter,
    /// Launcher filter chips above the installed games (0 = all).
    GameSource,
    GameScan,
    GameBrowse,
    GameRemove,
    GameButtons,
    GameName,
    GameExe,
    GameStage,
    GamePolling,
    PairStart,
    PairCancel,
    Unpair,
    AutoPair,
}

/// Which button layout the Customize page edits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Normal,
    Hypershift,
    /// Index into `cfg.games`.
    Game(usize),
}

/// True when a profile is for this installed game (same folder, or same program for a profile without one).
fn same_game(g: &GameConfig, l: &Game) -> bool {
    let gd = library::norm_dir(&g.dir);
    if !gd.is_empty() {
        return gd == library::norm_dir(&l.dir);
    }
    !l.exe.is_empty() && g.exe.eq_ignore_ascii_case(&l.exe)
}

/// A dongle pairing action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PairAct {
    Start,
    Cancel,
    Unpair,
    Auto(bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Body,
    Bold,
    Small,
    Caps,
    Title,
    Big,
}

pub enum Kind {
    Card,
    Line,
    Text { text: String, font: FontKind, color: u32, flags: u32 },
    Toggle { on: bool },
    /// Segmented control. `xs` are the option spans relative to `rect.left`.
    Chips { options: Vec<String>, selected: Option<usize>, xs: Vec<(i32, i32)> },
    Slider { min: i32, max: i32, step: i32, value: i32, fmt: fn(i32) -> String, ticks: Vec<String>, show_value: bool, enabled: bool },
    Button { text: String, primary: bool, enabled: bool, icon: Option<Icon> },
    Edit { text: String, numeric: bool },
    Radio { on: bool, text: String },
    /// A selectable list row (button list on the Customize page).
    Row { title: String, sub: String, selected: bool },
    Battery,
    MouseArt { highlight: usize, regions: Vec<(usize, RECT)> },
    /// Sidebar entry for a page.
    Nav { label: String, icon: Icon, selected: bool },
    /// Game tile with its cover art, name and launcher badge.
    Tile { title: String, sub: String, badge: &'static str, art: TileArt, selected: bool },
    /// A standalone icon glyph.
    Icon { icon: Icon, color: u32 },
    /// Small coloured pill with a caps label (feature status on the Roadmap page).
    Badge { text: String, color: u32 },
    /// Bullet dot in front of a roadmap item.
    Bullet { color: u32 },
    /// Dynamic Sensitivity plot: control points, the 250 samples, and whether the points can be dragged.
    Curve { points: Vec<(u8, f32)>, samples: Vec<f32>, editable: bool, selected: Option<usize> },
}

/// Where a game tile's picture comes from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TileArt {
    /// No picture: a generated tile.
    None,
    /// The dashed "add a program" tile.
    Add,
    /// Cover or program icon of an installed game (index into `Ui::library`).
    Lib(usize),
    /// Icon of this program (full path, loaded into `Ui::exe_icons`).
    Exe(String),
}

/// A game tile the Games page shows.
pub struct TileItem {
    pub id: Id,
    pub title: String,
    pub sub: String,
    pub badge: &'static str,
    pub art: TileArt,
    pub selected: bool,
}

pub struct Widget {
    pub id: Id,
    pub rect: RECT,
    pub kind: Kind,
    /// Header widgets do not scroll with the content.
    pub fixed: bool,
}

impl Widget {
    fn interactive(&self) -> bool {
        self.id != Id::None
            && !matches!(self.kind, Kind::Card | Kind::Line | Kind::Text { .. } | Kind::Edit { .. } | Kind::Battery | Kind::Icon { .. })
            && !matches!(self.kind, Kind::Button { enabled: false, .. } | Kind::Slider { enabled: false, .. })
    }
}

struct EditCtl {
    id: Id,
    hwnd: HWND,
    used: bool,
}

/// Which onboard setting a config change maps to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Write {
    None,
    Stages,
    Polling,
    Idle,
    LowBattery,
    Tracking,
    Rotation,
    DongleLed,
    Accel,
    Profile,
    Button(u8, bool),
    Macro(u16),
    MacroDelete(u16),
    Pair(PairAct),
}

pub struct Ui {
    hwnd: HWND,
    owner: HWND,
    pub(crate) scale: f32,
    fonts: Fonts,
    field_brush: HBRUSH,
    pub(crate) page: Page,
    pub(crate) cfg: Config,
    /// Last config that validated and was saved; a rejected change goes back to it.
    last_good: Config,
    cfg_error: Option<String>,
    /// Queue of the mouse I/O thread (see [`Dev`]); the window itself never waits for USB.
    io: Option<mpsc::Sender<Job>>,
    /// Jobs sent to the I/O thread and not finished yet.
    io_busy: usize,
    /// A battery/identity read, or a pending-change retry, is queued.
    reading: bool,
    retrying: bool,
    /// The mouse's HID interface was found at the last access.
    connected: bool,
    pub(crate) battery: Option<Battery>,
    pub(crate) fw: Option<(u8, u8)>,
    pub(crate) dongle_fw: Option<(u8, u8)>,
    pub(crate) serial: Option<String>,
    pub(crate) wireless: bool,
    /// What the device table says about the open mouse (the Viper V3 Pro until one is open).
    pub(crate) spec: DeviceSpec,
    pub(crate) asleep: bool,
    widgets: Vec<Widget>,
    edits: Vec<EditCtl>,
    hot: Option<usize>,
    pressed: Option<usize>,
    drag: Option<usize>,
    /// (widget, control point) being dragged on the acceleration curve.
    drag_pt: Option<(usize, usize)>,
    /// Selected control point of the custom acceleration curve.
    pub(crate) curve_sel: Option<usize>,
    tracking_leave: bool,
    scroll: i32,
    content_h: i32,
    status: Option<(String, u32, Instant)>,
    pub(crate) pending: bool,
    /// (stored, maximum) onboard profiles reported by the mouse.
    pub(crate) profiles: Option<(u8, u8)>,
    pub(crate) sel_button: usize,
    pub(crate) layer: Layer,
    /// Config key ("back", "hypershift.back") whose keyboard assignment is being typed.
    pub(crate) key_mode: Option<String>,
    /// Button key whose Text function is being typed (the edit box shows before a text exists).
    pub(crate) text_mode: Option<String>,
    pub(crate) autostart: bool,
    rotation_last: i8,
    /// Macro selected in the MACROS list (index into `cfg.macros`).
    pub(crate) sel_macro: Option<usize>,
    /// Recording in progress, and the macro id it is for.
    recording: Option<(record::Recorder, u16)>,
    pub(crate) rec_count: usize,
    pub(crate) rec_secs: u64,
    pub(crate) macro_storage: Option<MacroStorage>,
    /// Whether the mouse stores macros itself (false: the tray app plays them).
    pub(crate) macro_onboard: Option<bool>,
    /// Game profile selected on the Games page (index into `cfg.games`).
    pub(crate) sel_game: usize,
    /// Whether the selected profile's settings are shown under its row.
    pub(crate) game_open: bool,
    /// Widget to scroll to at the next layout (a newly created game profile).
    scroll_to: Option<Id>,
    /// Games found in the launchers (Steam, Epic, ...), filled by a background scan.
    pub(crate) library: Vec<Game>,
    /// Cover image and program icon per library entry (same indices).
    pub(crate) covers: Vec<Option<art::Cover>>,
    pub(crate) lib_icons: Vec<Option<art::ExeIcon>>,
    /// Program icons of profiles added by hand, by full path.
    pub(crate) exe_icons: HashMap<String, Option<art::ExeIcon>>,
    /// Guessed main program per install folder (profiles without a program name).
    exe_guess: HashMap<String, Option<String>>,
    pub(crate) scanning: bool,
    /// Text typed in the game list's search box.
    pub(crate) game_filter: String,
    /// Launcher the installed games list is filtered to (None = all).
    pub(crate) lib_source: Option<&'static str>,
    pub(crate) paired: Option<Vec<PairedDevice>>,
    pub(crate) auto_pair: Option<bool>,
    /// When pairing mode was started, until it is cancelled.
    pub(crate) pairing_since: Option<Instant>,
    /// "Unpair" was clicked once; the next click does it.
    pub(crate) confirm_unpair: bool,
}

/// Brand-like dot colour for a launcher badge.
fn badge_color(badge: &str) -> u32 {
    match badge {
        "STEAM" => rgb(0x66, 0xc0, 0xf4),
        "XBOX" => rgb(0x52, 0xb0, 0x43),
        "EPIC GAMES" => WHITE,
        "GOG" => rgb(0xc9, 0x86, 0xd9),
        "EA APP" => rgb(0xff, 0x6b, 0x6b),
        "BATTLE.NET" => rgb(0x14, 0x8e, 0xff),
        "RIOT" => rgb(0xd1, 0x36, 0x39),
        "UBISOFT CONNECT" => rgb(0x3b, 0x8b, 0xff),
        "ROCKSTAR" => rgb(0xfc, 0xaf, 0x17),
        _ => MUTED,
    }
}

/// Debug trace of the input handling, written to the file named by `FREENAPSE_TRACE` when set.
fn trace(msg: &str) {
    static PATH: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    let Some(p) = PATH.get_or_init(|| std::env::var_os("FREENAPSE_TRACE").map(std::path::PathBuf::from)) else { return };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(p) {
        let _ = writeln!(f, "{msg}");
    }
}

/// Original window procedure of the EDIT class, saved when the first box is subclassed.
static OLD_EDIT_PROC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Set while a window thread is starting, so a double click opens one window.
static OPENING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Open the settings window, or bring the open one forward. The window runs on
/// its own thread, so the tray app's device reads never hold up painting or clicks.
pub unsafe fn open(owner: HWND) {
    let existing = FindWindowExW(0, 0, wide(CLASS_NAME).as_ptr(), ptr::null());
    if existing != 0 {
        ShowWindow(existing, SW_RESTORE);
        SetForegroundWindow(existing);
        return;
    }
    if OPENING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || unsafe {
        let hwnd = create(owner);
        OPENING.store(false, std::sync::atomic::Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        // Ends with PostQuitMessage in WM_DESTROY.
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe fn create(owner: HWND) -> HWND {
    let class = wide(CLASS_NAME);
    let hinstance = GetModuleHandleW(ptr::null());
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: {
            // Resource 1 is the icon embedded by build.rs; fall back to the stock one.
            let own = LoadIconW(hinstance, 1 as *const u16);
            if own != 0 { own } else { LoadIconW(0, IDI_APPLICATION) }
        },
        hCursor: LoadCursorW(0, IDC_ARROW),
        hbrBackground: 0,
        lpszMenuName: ptr::null(),
        lpszClassName: class.as_ptr(),
    };
    RegisterClassW(&wc);

    let ui = Box::new(Ui {
        hwnd: 0,
        owner,
        scale: 1.0,
        fonts: Fonts::new(1.0),
        field_brush: CreateSolidBrush(FIELD),
        page: Page::Performance,
        cfg: Config::default(),
        last_good: Config::default(),
        cfg_error: None,
        io: None,
        io_busy: 0,
        reading: false,
        retrying: false,
        connected: false,
        battery: None,
        fw: None,
        dongle_fw: None,
        serial: None,
        wireless: true,
        spec: DeviceSpec::default(),
        asleep: false,
        widgets: Vec::new(),
        edits: Vec::new(),
        hot: None,
        pressed: None,
        drag: None,
        drag_pt: None,
        curve_sel: None,
        tracking_leave: false,
        scroll: 0,
        content_h: 0,
        status: None,
        pending: false,
        profiles: None,
        sel_button: 0,
        layer: Layer::Normal,
        key_mode: None,
        text_mode: None,
        autostart: autostart::is_enabled(),
        rotation_last: 5,
        sel_macro: None,
        recording: None,
        rec_count: 0,
        rec_secs: 0,
        macro_storage: None,
        macro_onboard: None,
        sel_game: 0,
        game_open: false,
        scroll_to: None,
        library: Vec::new(),
        covers: Vec::new(),
        lib_icons: Vec::new(),
        exe_icons: HashMap::new(),
        exe_guess: HashMap::new(),
        scanning: false,
        game_filter: String::new(),
        lib_source: None,
        paired: None,
        auto_pair: None,
        pairing_since: None,
        confirm_unpair: false,
    });
    let p = Box::into_raw(ui);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        wide("freenapse").as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        WIN_W,
        WIN_H,
        0,
        0,
        hinstance,
        p as *const c_void,
    );
    if hwnd == 0 {
        drop(Box::from_raw(p));
        return 0;
    }
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    hwnd
}

unsafe fn ui_from(hwnd: HWND) -> *mut Ui {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui
}

fn lo_x(lparam: LPARAM) -> i32 {
    (lparam & 0xffff) as u16 as i16 as i32
}
fn hi_y(lparam: LPARAM) -> i32 {
    ((lparam >> 16) & 0xffff) as u16 as i16 as i32
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CREATE {
        let cs = lparam as *const CREATESTRUCTW;
        let ui = (*cs).lpCreateParams as *mut Ui;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ui as isize);
        (*ui).hwnd = hwnd;
        (*ui).created();
        return 0;
    }
    let ui = ui_from(hwnd);
    if ui.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ui = &mut *ui;
    match msg {
        WM_PAINT => {
            ui.paint();
            0
        }
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            if wparam as u32 != SIZE_MINIMIZED {
                ui.layout();
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_GETMINMAXINFO => {
            let mmi = lparam as *mut MINMAXINFO;
            (*mmi).ptMinTrackSize.x = ui.s(MIN_W);
            (*mmi).ptMinTrackSize.y = ui.s(MIN_H);
            0
        }
        WM_DPICHANGED => {
            ui.set_dpi((wparam & 0xffff) as u32);
            let r = &*(lparam as *const RECT);
            SetWindowPos(hwnd, 0, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            ui.layout();
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_LBUTTONDOWN => {
            SetFocus(hwnd);
            ui.mouse_down(lo_x(lparam), hi_y(lparam));
            0
        }
        WM_LBUTTONUP => {
            ui.mouse_up(lo_x(lparam), hi_y(lparam));
            0
        }
        WM_MOUSEMOVE => {
            ui.mouse_move(lo_x(lparam), hi_y(lparam));
            0
        }
        WM_MOUSELEAVE => {
            ui.tracking_leave = false;
            if ui.hot.take().is_some() {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xffff) as u16 as i16 as i32;
            if let Some((rec, _)) = ui.recording.as_mut() {
                rec.scroll(delta > 0);
                return 0;
            }
            ui.scroll_by(-delta / 120 * ui.s(48));
            0
        }
        WM_SETCURSOR => {
            if (lparam & 0xffff) as u32 == HTCLIENT {
                let hand = ui.hot.is_some_and(|i| ui.widgets[i].interactive());
                SetCursor(LoadCursorW(0, if hand { IDC_HAND } else { IDC_ARROW }));
                return 1;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND => {
            let code = (wparam >> 16) as u32;
            if code == EN_KILLFOCUS {
                ui.edit_committed(lparam as HWND);
            } else if code == EN_CHANGE {
                ui.edit_changed(lparam as HWND);
            }
            0
        }
        WM_LIBRARY => {
            let games = Box::from_raw(lparam as *mut Scanned);
            ui.library_loaded(*games);
            0
        }
        WM_CTLCOLOREDIT => {
            let dc = wparam as HDC;
            SetTextColor(dc, TEXT);
            SetBkColor(dc, FIELD);
            ui.field_brush as LRESULT
        }
        WM_TIMER => {
            match wparam {
                TIMER_RECORD => ui.record_tick(),
                TIMER_TICK => ui.tick(),
                TIMER_BATTERY => {
                    if user_active() {
                        ui.read_live(false);
                        ui.layout();
                        InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
                _ => {}
            }
            0
        }
        WM_KEYDOWN => {
            if wparam as u16 == VK_ESCAPE {
                if ui.recording.is_some() {
                    ui.stop_recording();
                } else {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_TICK);
            KillTimer(hwnd, TIMER_BATTERY);
            KillTimer(hwnd, TIMER_RECORD);
            if ui.pairing_since.is_some() {
                // Runs after the window is gone; its result has nowhere to go and is dropped.
                ui.io(|dev| {
                    if let Ok(m) = dev.mouse() {
                        let _ = m.cancel_pairing(m.spec().product_id);
                    }
                    Box::new(|_| {})
                });
            }
            ui.fonts.free();
            DeleteObject(ui.field_brush);
            let owner = ui.owner;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            // Dropping the queue lets the I/O thread finish what is queued and exit.
            drop(Box::from_raw(ui as *mut Ui));
            if owner != 0 {
                PostMessageW(owner, WM_UI_CLOSED, 0, 0);
            }
            // The window has its own thread: end its message loop.
            PostQuitMessage(0);
            0
        }
        WM_IO_DONE => {
            let done = Box::from_raw(lparam as *mut Done);
            ui.io_busy = ui.io_busy.saturating_sub(1);
            (*done)(ui);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_KEYDOWN && (wparam as u16 == VK_RETURN || wparam as u16 == VK_ESCAPE) {
        SetFocus(GetParent(hwnd));
        return 0;
    }
    if msg == WM_CHAR && (wparam == 13 || wparam == 27) {
        return 0;
    }
    let old = OLD_EDIT_PROC.load(std::sync::atomic::Ordering::Relaxed);
    CallWindowProcW(mem::transmute::<isize, WNDPROC>(old), hwnd, msg, wparam, lparam)
}

impl Ui {
    /// Logical pixels to device pixels.
    pub(crate) fn s(&self, v: i32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    unsafe fn set_dpi(&mut self, dpi: u32) {
        self.scale = dpi as f32 / 96.0;
        self.fonts.free();
        self.fonts = Fonts::new(self.scale);
        for e in &self.edits {
            SendMessageW(e.hwnd, WM_SETFONT, self.fonts.body as usize, 1);
        }
    }

    unsafe fn created(&mut self) {
        let dark: i32 = 1;
        DwmSetWindowAttribute(self.hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &dark as *const i32 as *const c_void, 4);
        let caption: u32 = PANEL;
        DwmSetWindowAttribute(self.hwnd, DWMWA_CAPTION_COLOR as u32, &caption as *const u32 as *const c_void, 4);
        // Windows 11 rounds the window; ask for it explicitly (ignored on Windows 10).
        let round_pref: i32 = 2; // DWMWCP_ROUND
        DwmSetWindowAttribute(self.hwnd, 33, &round_pref as *const i32 as *const c_void, 4); // DWMWA_WINDOW_CORNER_PREFERENCE

        self.set_dpi(GetDpiForWindow(self.hwnd));
        // As large as a game client, but never beyond the work area.
        let mut work = RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) };
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut RECT as *mut c_void, 0);
        let (ww, wh) = (work.right - work.left, work.bottom - work.top);
        let w = self.s(WIN_W).min(ww - self.s(24)).max(self.s(MIN_W).min(ww));
        let h = self.s(WIN_H).min(wh - self.s(24)).max(self.s(MIN_H).min(wh));
        SetWindowPos(self.hwnd, 0, work.left + ((ww - w) / 2).max(0), work.top + ((wh - h) / 2).max(0), w, h, SWP_NOZORDER | SWP_NOACTIVATE);

        self.start_io();
        self.load_config();
        self.read_live(true);
        self.start_scan();
        self.layout();
        SetTimer(self.hwnd, TIMER_TICK, 1000, None);
        SetTimer(self.hwnd, TIMER_BATTERY, 30_000, None);
    }

    fn load_config(&mut self) {
        let path = config_path();
        if path.exists() {
            match Config::load(&path) {
                Ok(c) => {
                    self.cfg = c;
                    self.cfg_error = None;
                }
                Err(e) => {
                    self.cfg = Config::default();
                    self.cfg_error = Some(e);
                }
            }
        } else if let Ok(m) = Mouse::open() {
            // First run: start from what the mouse has stored, like the tray app does
            // (once, before there is anything to show).
            if let Ok(cfg) = apply::import(&m) {
                let _ = Config::write_new(&path, &cfg);
                self.cfg = cfg;
            }
        }
        if self.cfg.sensor.rotation != 0 {
            self.rotation_last = self.cfg.sensor.rotation;
        }
        self.last_good = self.cfg.clone();
        if let Some(e) = self.cfg_error.clone() {
            self.set_status(&format!("Config file error: {e}"), ERR);
        }
    }

    /// Battery, firmware and connection state. `full` also reads identity once.
    fn read_live(&mut self, full: bool) {
        if self.reading {
            return;
        }
        self.reading = true;
        let want_ident = full || self.fw.is_none();
        let want_pair = full || self.paired.is_none();
        // Battery first, so the header updates after one exchange; identity and pairing follow.
        self.io(move |dev| {
            let res = dev.mouse().and_then(|m| m.battery());
            dev.check(&res);
            let spec = dev.mouse.as_ref().map(|m| m.spec().clone());
            Box::new(move |ui: &mut Ui| {
                if let Some(spec) = spec {
                    ui.wireless = spec.wireless;
                    ui.spec = spec;
                }
                let awake = res.is_ok();
                match res {
                    Ok(b) => {
                        ui.connected = true;
                        ui.battery = Some(b);
                        ui.asleep = false;
                    }
                    Err(e) if is_gone(&e) => {
                        ui.connected = false;
                        ui.battery = None;
                    }
                    Err(_) => {
                        ui.connected = true;
                        ui.asleep = true;
                    }
                }
                if awake && (want_ident || want_pair) {
                    ui.read_details(want_ident, want_pair);
                } else {
                    ui.reading = false;
                }
                ui.repaint();
            })
        });
    }

    /// Firmware, serial, profiles, macro memory and the dongle's pairings.
    fn read_details(&mut self, ident: bool, pair: bool) {
        self.io(move |dev| {
            let Ok(m) = dev.mouse() else { return Box::new(|ui: &mut Ui| ui.reading = false) };
            let ident = ident.then(|| (m.firmware().ok(), m.dongle_firmware().ok(), m.serial().ok(), m.profiles().ok(), m.macro_storage().ok(), m.macro_support().ok()));
            let paired = if pair && m.is_wireless() { Some(m.paired_devices().ok()) } else { None };
            let auto = if paired.is_some() { dev.auto_pairing() } else { None };
            Box::new(move |ui: &mut Ui| {
                ui.reading = false;
                if let Some((fw, dfw, serial, profiles, storage, onboard)) = ident {
                    ui.fw = fw;
                    ui.dongle_fw = dfw;
                    ui.serial = serial;
                    ui.profiles = profiles;
                    ui.macro_storage = storage;
                    ui.macro_onboard = onboard;
                }
                if let Some(p) = paired {
                    ui.paired = p;
                    ui.auto_pair = auto;
                }
                ui.repaint();
            })
        });
    }

    /// Start the mouse I/O thread; its results come back as WM_IO_DONE.
    fn start_io(&mut self) {
        let (tx, rx) = mpsc::channel::<Job>();
        let hwnd = self.hwnd;
        std::thread::spawn(move || {
            let mut dev = Dev { mouse: None, no_auto_pair: false };
            for job in rx {
                let done = Box::into_raw(Box::new(job(&mut dev)));
                if unsafe { PostMessageW(hwnd, WM_IO_DONE, 0, done as LPARAM) } == 0 {
                    drop(unsafe { Box::from_raw(done) });
                }
            }
        });
        self.io = Some(tx);
    }

    /// Queue work for the mouse I/O thread. Jobs run one at a time, in order.
    fn io(&mut self, job: impl FnOnce(&mut Dev) -> Done + Send + 'static) {
        if let Some(tx) = &self.io {
            if tx.send(Box::new(job)).is_ok() {
                self.io_busy += 1;
            }
        }
    }

    fn repaint(&mut self) {
        unsafe {
            self.layout();
            InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    pub(crate) fn set_status(&mut self, text: &str, color: u32) {
        self.status = Some((text.to_string(), color, Instant::now()));
    }

    unsafe fn tick(&mut self) {
        let mut repaint = false;
        if self.pending && user_active() {
            self.retry_pending();
        }
        if let Some(since) = self.pairing_since {
            if since.elapsed().as_secs() >= PAIRING_SECS {
                self.pairing_since = None;
                self.io(|dev| {
                    if let Ok(m) = dev.mouse() {
                        let _ = m.cancel_pairing(m.spec().product_id);
                    }
                    Box::new(|_| {})
                });
                self.set_status("Pairing mode ended (no device paired within 90 s)", WARN);
                repaint = true;
            } else if self.io_busy == 0 {
                // Poll the dongle: a newly paired device shows up in its list.
                self.io(|dev| {
                    let p = dev.mouse().and_then(|m| m.paired_devices());
                    Box::new(move |ui: &mut Ui| {
                        if let Ok(p) = p {
                            if ui.paired.as_ref() != Some(&p) {
                                ui.paired = Some(p);
                                ui.repaint();
                            }
                        }
                    })
                });
            }
        }
        let expired = self.status.as_ref().is_some_and(|(_, _, at)| at.elapsed().as_secs() >= 8);
        if expired && !self.pending {
            self.status = None;
            repaint = true;
        }
        if repaint {
            self.layout();
            InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    // ------------------------------------------------------------------
    // Layout
    // ------------------------------------------------------------------

    fn client_size(&self) -> (i32, i32) {
        let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { GetClientRect(self.hwnd, &mut rc) };
        (rc.right, rc.bottom)
    }

    fn content_top(&self) -> i32 {
        self.s(TOPBAR_H)
    }

    /// Left edge of the page content (right of the sidebar).
    fn content_left(&self) -> i32 {
        self.s(SIDEBAR_W)
    }

    fn view_h(&self) -> i32 {
        let (_, h) = self.client_size();
        (h - self.s(TOPBAR_H) - self.s(FOOTER_H)).max(1)
    }

    unsafe fn layout(&mut self) {
        trace("layout");
        let (w, h) = self.client_size();
        if self.page == Page::Games {
            self.warm_icons();
        }
        let dc = GetDC(self.hwnd);
        self.widgets.clear();

        // Navigation pane: one entry per page, Settings above the device card at the bottom.
        let nav_x = self.s(NAV_PAD);
        let nav_w = self.s(SIDEBAR_W - 2 * NAV_PAD);
        let nav_h = self.s(NAV_H);
        let mut y = self.s(80);
        for (i, p) in Page::TABS.iter().enumerate() {
            if *p == Page::Calibration && !self.spec.sensor_tracking {
                continue; // the tracking page of src/sensor.rs is not known to apply to this mouse
            }
            self.widgets.push(Widget {
                id: Id::Tab(i),
                rect: rect(nav_x, y, nav_w, nav_h),
                kind: Kind::Nav { label: p.label().to_string(), icon: p.icon(), selected: self.page == *p },
                fixed: true,
            });
            y += nav_h + self.s(4);
        }
        self.widgets.push(Widget {
            id: Id::Gear,
            rect: rect(nav_x, h - self.s(FOOTER_H) - self.s(DEVICE_CARD_H) - nav_h - self.s(12), nav_w, nav_h),
            kind: Kind::Nav { label: Page::App.label().to_string(), icon: Page::App.icon(), selected: self.page == Page::App },
            fixed: true,
        });

        let margin = self.content_left() + self.s(MARGIN);
        let mut avail = (w - margin - self.s(MARGIN)).max(self.s(200));
        if self.page != Page::Games {
            avail = avail.min(self.s(CONTENT_MAX_W));
        }
        let mut form = pages::Form::new(self.scale, dc, &self.fonts, margin, avail);
        pages::build(self, &mut form);
        let (widgets, height) = form.finish();
        self.widgets.extend(widgets);
        self.content_h = height + self.s(16);
        ReleaseDC(self.hwnd, dc);

        let max_scroll = (self.content_h - self.view_h()).max(0);
        if let Some(id) = self.scroll_to.take() {
            if let Some(w) = self.widgets.iter().find(|w| w.id == id && !w.fixed) {
                self.scroll = w.rect.top - self.s(24);
            }
        }
        self.scroll = self.scroll.clamp(0, max_scroll);
        self.hot = None;
        self.sync_edits();
    }

    unsafe fn sync_edits(&mut self) {
        for e in &mut self.edits {
            e.used = false;
        }
        let hinstance = GetModuleHandleW(ptr::null());
        let view_top = self.content_top();
        let view_bot = view_top + self.view_h();
        let top = view_top - self.scroll;
        let focus = GetFocus();
        let edit_h = self.s(22);
        for wgt in &self.widgets {
            let Kind::Edit { text, numeric } = &wgt.kind else { continue };
            let idx = match self.edits.iter().position(|e| e.id == wgt.id) {
                Some(i) => i,
                None => {
                    let style = WS_CHILD | WS_VISIBLE | (ES_AUTOHSCROLL | if *numeric { ES_NUMBER } else { 0 }) as u32;
                    let h = CreateWindowExW(0, wide("EDIT").as_ptr(), ptr::null(), style, 0, 0, 10, 10, self.hwnd, 0, hinstance, ptr::null());
                    SendMessageW(h, WM_SETFONT, self.fonts.body as usize, 1);
                    if wgt.id == Id::GameFilter {
                        // EM_SETCUEBANNER: placeholder text while the box is empty.
                        SendMessageW(h, 0x1501, 1, wide("Search games").as_ptr() as isize);
                    }
                    let old = SetWindowLongPtrW(h, GWLP_WNDPROC, edit_proc as *const () as usize as isize);
                    if OLD_EDIT_PROC.load(std::sync::atomic::Ordering::Relaxed) == 0 {
                        OLD_EDIT_PROC.store(old, std::sync::atomic::Ordering::Relaxed);
                    }
                    self.edits.push(EditCtl { id: wgt.id, hwnd: h, used: true });
                    self.edits.len() - 1
                }
            };
            let (pad, pad2) = (self.s(10), self.s(20));
            let e = &mut self.edits[idx];
            e.used = true;
            let r = &wgt.rect;
            let y = r.top + (r.bottom - r.top - edit_h) / 2;
            // Child controls are not clipped by our content clip: hide the ones scrolled out of view.
            if y + top < view_top || y + top + edit_h > view_bot {
                ShowWindow(e.hwnd, SW_HIDE);
                continue;
            }
            MoveWindow(e.hwnd, r.left + pad, y + top, r.right - r.left - pad2, edit_h, 1);
            if focus != e.hwnd {
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(e.hwnd, buf.as_mut_ptr(), buf.len() as i32);
                let cur = String::from_utf16_lossy(&buf[..n as usize]);
                if cur != *text {
                    SetWindowTextW(e.hwnd, wide(text).as_ptr());
                }
            }
            ShowWindow(e.hwnd, SW_SHOW);
        }
        for e in &self.edits {
            if !e.used {
                ShowWindow(e.hwnd, SW_HIDE);
            }
        }
    }

    unsafe fn scroll_by(&mut self, dy: i32) {
        let max_scroll = (self.content_h - self.view_h()).max(0);
        let new = (self.scroll + dy).clamp(0, max_scroll);
        if new != self.scroll {
            self.scroll = new;
            self.sync_edits();
            InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    // ------------------------------------------------------------------
    // Painting
    // ------------------------------------------------------------------

    fn font(&self, f: FontKind) -> HFONT {
        match f {
            FontKind::Body => self.fonts.body,
            FontKind::Bold => self.fonts.bold,
            FontKind::Small => self.fonts.small,
            FontKind::Caps => self.fonts.caps,
            FontKind::Title => self.fonts.title,
            FontKind::Big => self.fonts.big,
        }
    }

    unsafe fn paint(&mut self) {
        let mut ps: PAINTSTRUCT = mem::zeroed();
        let hdc = BeginPaint(self.hwnd, &mut ps);
        let (w, h) = self.client_size();
        let mem_dc = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, w.max(1), h.max(1));
        let old = SelectObject(mem_dc, bmp);
        self.draw_all(mem_dc, w, h);
        BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
        SelectObject(mem_dc, old);
        DeleteObject(bmp);
        DeleteDC(mem_dc);
        EndPaint(self.hwnd, &ps);
    }

    unsafe fn draw_all(&self, dc: HDC, w: i32, h: i32) {
        let s = |v: i32| self.s(v);
        fill(dc, &rect(0, 0, w, h), BG);

        // Scrolling content, clipped to the column between the page header and the status line.
        let top = self.content_top();
        let left = self.content_left();
        let view = rect(left, top, w - left, self.view_h());
        fill(dc, &view, PANEL);
        let clip = CreateRectRgn(view.left, view.top, view.right, view.bottom);
        SelectClipRgn(dc, clip);
        DeleteObject(clip);
        for (i, wgt) in self.widgets.iter().enumerate() {
            if wgt.fixed {
                continue;
            }
            let r = offset(&wgt.rect, 0, top - self.scroll);
            if r.bottom < view.top || r.top > view.bottom {
                continue;
            }
            self.draw_widget(dc, wgt, &r, self.hot == Some(i), self.pressed == Some(i));
        }
        SelectClipRgn(dc, 0);

        // Scrollbar hint.
        if self.content_h > self.view_h() {
            let track_h = self.view_h();
            let bar_h = (track_h * track_h / self.content_h).max(s(24));
            let bar_y = top + (track_h - bar_h) * self.scroll / (self.content_h - track_h).max(1);
            round(dc, &rect(w - s(8), bar_y, s(4), bar_h), s(2), Some(TRACK), None);
        }

        self.draw_topbar(dc, w);
        self.draw_footer(dc, w, h);
        self.draw_sidebar(dc, h);
        for (i, wgt) in self.widgets.iter().enumerate() {
            if wgt.fixed {
                self.draw_widget(dc, wgt, &wgt.rect, self.hot == Some(i), self.pressed == Some(i));
            }
        }
    }

    /// Page header: icon, title and the one-line subtitle (DESIGN.md 4.4). Device
    /// status lives in the device card only, so there are no pills here.
    unsafe fn draw_topbar(&self, dc: HDC, w: i32) {
        let s = |v: i32| self.s(v);
        let left = self.content_left();
        let r = rect(left, 0, w - left, s(TOPBAR_H));
        fill(dc, &r, BG);
        let x = left + s(MARGIN);
        icon(dc, self.fonts.icon_lg, x + s(12), s(38), self.page.icon(), MUTED);
        text(dc, &rect(x + s(36), s(18), w / 2, s(36)), self.page.label(), self.fonts.title, TEXT, DT_ROW);
        text(dc, &rect(x + s(37), s(56), w - x - s(48), s(20)), self.page.subtitle(), self.fonts.body, MUTED, DT_ROW);
    }

    /// Status line under the content (DESIGN.md 4.14): empty at rest, a message
    /// with an icon after a write to the mouse or while a change is pending.
    unsafe fn draw_footer(&self, dc: HDC, w: i32, h: i32) {
        let s = |v: i32| self.s(v);
        let left = self.content_left();
        let fy = h - s(FOOTER_H);
        fill(dc, &rect(left, fy, w - left, s(FOOTER_H)), PANEL);
        let (msg, color, ic) = match &self.status {
            Some((m, c, _)) => (m.clone(), *c, if *c == ERR { Icon::Warning } else if *c == WARN { Icon::Info } else { Icon::Check }),
            None if self.pending => ("Changes pending: move the mouse to wake it up".to_string(), WARN, Icon::Info),
            None => return,
        };
        let x = left + s(MARGIN);
        icon(dc, self.fonts.icon_sm, x + s(6), fy + s(FOOTER_H) / 2, ic, color);
        text(dc, &rect(x + s(22), fy, w - x - s(40), s(FOOTER_H)), &msg, self.fonts.small, color, DT_ROW);
    }

    /// Navigation pane (DESIGN.md 4.2): wordmark, the items (drawn as widgets)
    /// and the device card, which is the one place that shows link and battery.
    unsafe fn draw_sidebar(&self, dc: HDC, h: i32) {
        let s = |v: i32| self.s(v);
        let sw = s(SIDEBAR_W);
        fill(dc, &rect(0, 0, sw, h), BG);
        fill(dc, &rect(sw - 1, 0, 1, h), LINE);

        // Wordmark: an accent tile with the logo mark, the name and the tagline.
        let cy = s(36);
        let tile = rect(s(16), cy - s(12), s(24), s(24));
        round(dc, &tile, s(5), Some(ACCENT), None);
        logo_mark(dc, tile.left, tile.top, s(24), ON_ACCENT, ON_ACCENT);
        text(dc, &rect(s(50), cy - s(14), sw - s(60), s(24)), "freenapse", self.fonts.logo, TEXT, DT_ROW);
        text(dc, &rect(s(51), cy + s(8), sw - s(60), s(16)), "Mouse control", self.fonts.small, DIM, DT_ROW);

        // Device card.
        let ch = s(DEVICE_CARD_H);
        let cr = rect(s(NAV_PAD), h - s(FOOTER_H) - ch - s(4), sw - s(2 * NAV_PAD), ch);
        card(dc, &cr, s(4));
        let online = self.connected && !self.asleep;
        let model = self.spec.name.trim_start_matches("Razer ").split(" (").next().unwrap_or("Mouse").to_string();
        text(dc, &rect(cr.left + s(16), cr.top + s(12), cr.right - cr.left - s(52), s(20)), &model, self.fonts.bold, TEXT, DT_ROW);
        let (licon, lcol) = match (self.connected, self.asleep, self.wireless) {
            (false, _, _) => (Icon::Warning, DIM),
            (true, true, _) => (Icon::Wifi, WARN),
            (true, false, true) => (Icon::Wifi, ACCENT),
            (true, false, false) => (Icon::Cable, ACCENT),
        };
        icon(dc, self.fonts.icon, cr.right - s(24), cr.top + s(22), licon, lcol);
        let sub = match (self.connected, self.asleep) {
            (false, _) => "Not connected".to_string(),
            (true, true) => "Asleep, move it to wake".to_string(),
            (true, false) => match (self.fw, self.dongle_fw) {
                (Some((a, b)), Some((c, d))) => format!("fw {a}.{b}  ·  dongle {c}.{d}"),
                (Some((a, b)), None) => format!("Firmware {a}.{b}"),
                _ => if self.wireless { "HyperSpeed wireless".to_string() } else { "Wired".to_string() },
            },
        };
        text(dc, &rect(cr.left + s(16), cr.top + s(32), cr.right - cr.left - s(32), s(16)), &sub, self.fonts.small, MUTED, DT_ROW);
        // Battery bar with the percentage at its right.
        let (bcol, pct, label) = match self.battery {
            Some(b) if b.charging => (BLUE, b.percent as i32, format!("{}% charging", b.percent)),
            Some(b) if b.percent as u32 <= self.cfg.tray.notify_percent as u32 => (WARN, b.percent as i32, format!("{}%", b.percent)),
            Some(b) => (ACCENT, b.percent as i32, format!("{}%", b.percent)),
            None => (DIM, 0, "--".to_string()),
        };
        let _ = online;
        let lw = s(72);
        let by = cr.bottom - s(22);
        let bar = rect(cr.left + s(16), by, cr.right - cr.left - s(32) - lw, s(4));
        round(dc, &bar, s(2), Some(TRACK), None);
        let fw = ((bar.right - bar.left) * pct / 100).max(if pct > 0 { s(4) } else { 0 });
        if fw > 0 {
            round(dc, &rect(bar.left, bar.top, fw, s(4)), s(2), Some(bcol), None);
        }
        text(dc, &rect(bar.right, by - s(6), lw - s(2), s(16)), &label, self.fonts.caps, if pct > 0 { TEXT } else { DIM }, DT_VCENTER_ | DT_SINGLELINE_ | DT_NOPREFIX_ | 2);
    }

    /// True while the EDIT box behind this widget has the keyboard focus.
    fn edit_focused(&self, id: Id) -> bool {
        let focus = unsafe { GetFocus() };
        self.edits.iter().any(|e| e.used && e.id == id && e.hwnd == focus)
    }

    unsafe fn draw_widget(&self, dc: HDC, wgt: &Widget, r: &RECT, hot: bool, pressed: bool) {
        let s = |v: i32| self.s(v);
        match &wgt.kind {
            Kind::Card => card(dc, r, s(4)),
            Kind::Line => fill(dc, r, DIVIDER),
            Kind::Text { text: t, font, color, flags } => text(dc, r, t, self.font(*font), *color, *flags),
            Kind::Icon { icon: ic, color } => icon(dc, self.fonts.icon, (r.left + r.right) / 2, (r.top + r.bottom) / 2, *ic, *color),
            Kind::Toggle { on } => {
                // DESIGN.md 4.8: 20 x 40 pill; off = outline, on = accent fill.
                let h = r.bottom - r.top;
                if *on {
                    round(dc, r, h / 2, Some(if hot { ACCENT_HI } else { ACCENT }), None);
                } else {
                    round(dc, r, h / 2, Some(if hot { FIELD_HI } else { FIELD }), Some((if hot { MUTED } else { TRACK }, s(1))));
                }
                let kr = if hot { s(7) } else { s(6) };
                let cx = if *on { r.right - s(4) - s(6) } else { r.left + s(4) + s(6) };
                circle(dc, cx, r.top + h / 2, kr, Some(if *on { ON_ACCENT } else { MUTED }), None);
            }
            Kind::Chips { options, selected, xs } => {
                // DESIGN.md 4.9: flat chips, the selected one filled with the accent.
                let hot_i = if hot { self.chip_at(wgt, self.cursor_x_in(wgt)) } else { None };
                let h = r.bottom - r.top;
                for (i, (x0, x1)) in xs.iter().enumerate() {
                    let cr = rect(r.left + x0, r.top, x1 - x0, h);
                    if *selected == Some(i) {
                        round(dc, &cr, s(4), Some(if hot_i == Some(i) { ACCENT_HI } else { ACCENT }), None);
                        text(dc, &cr, &options[i], self.fonts.bold, ON_ACCENT, DT_MID);
                    } else {
                        let (bg, fg, border) = if hot_i == Some(i) { (FIELD_HI, TEXT, TRACK) } else { (FIELD, TEXT, LINE) };
                        round(dc, &cr, s(4), Some(bg), Some((border, s(1))));
                        text(dc, &cr, &options[i], self.fonts.body, fg, DT_MID);
                    }
                }
            }
            Kind::Slider { min, max, step: _, value, fmt, ticks, show_value, enabled } => {
                // DESIGN.md 4.10: 4 px track, accent fill, 20 px knob with a card-coloured ring.
                let (tl, tr) = self.track_span(wgt, r);
                let cy = if ticks.is_empty() { (r.top + r.bottom) / 2 } else { r.top + s(16) };
                let frac = if max > min { (*value - *min) as f32 / (*max - *min) as f32 } else { 0.0 };
                let kx = tl + ((tr - tl) as f32 * frac).round() as i32;
                let (hot, pressed) = if *enabled { (hot, pressed) } else { (false, false) };
                let (knob_c, val_c) = if *enabled { (if hot || pressed { ACCENT_HI } else { ACCENT }, TEXT) } else { (TRACK, DIM) };
                round(dc, &rect(tl, cy - s(2), tr - tl, s(4)), s(2), Some(TRACK), None);
                let fr = rect(tl, cy - s(2), (kx - tl).max(s(4)), s(4));
                round(dc, &fr, s(2), Some(if *enabled { ACCENT } else { DIM }), None);
                let kr = if hot || pressed { s(11) } else { s(10) };
                circle(dc, kx, cy, kr, Some(CARD), None);
                circle(dc, kx, cy, kr - s(3), Some(knob_c), None);
                if *show_value {
                    text(dc, &rect(tr + s(14), cy - s(12), r.right - tr - s(14), s(24)), &fmt(*value), self.fonts.bold, val_c, DT_ROW);
                }
                if !ticks.is_empty() {
                    let n = ticks.len().max(2) as i32;
                    for (i, t) in ticks.iter().enumerate() {
                        let x = tl + (tr - tl) * i as i32 / (n - 1);
                        fill(dc, &rect(x, cy + s(8), s(1), s(4)), TRACK);
                        let active = i as i32 == *value - *min;
                        let tw = text_width(dc, self.fonts.small, t);
                        text(dc, &rect(x - tw / 2 - s(4), cy + s(14), tw + s(8), s(16)), t, if active { self.fonts.caps } else { self.fonts.small }, if active { TEXT } else { DIM }, DT_MID);
                    }
                }
            }
            Kind::Button { text: t, primary, enabled, icon: ic } => {
                // DESIGN.md 4.7: flat primary (accent, dark text) and standard (field, stroke) buttons.
                let h = r.bottom - r.top;
                let radius = s(4);
                let destructive = matches!(ic, Some(Icon::Trash));
                let fg = match (*enabled, *primary) {
                    (false, _) => {
                        round(dc, r, radius, Some(FIELD), None);
                        DIM
                    }
                    (true, true) => {
                        round(dc, r, radius, Some(if pressed { ACCENT_DK } else if hot { ACCENT_HI } else { ACCENT }), None);
                        ON_ACCENT
                    }
                    (true, false) => {
                        let (bg, fg) = if pressed { (FIELD, MUTED) } else if hot { (FIELD_HI, TEXT) } else { (FIELD, TEXT) };
                        glass(dc, r, radius, mix(WHITE, bg, 0.03), bg, TRACK, 0);
                        if destructive { ERR } else { fg }
                    }
                };
                match ic {
                    Some(ic) => {
                        let tw = text_width(dc, self.fonts.bold, t);
                        let x0 = r.left + (r.right - r.left - tw - s(24)) / 2;
                        icon(dc, self.fonts.icon, x0 + s(8), r.top + h / 2, *ic, fg);
                        text(dc, &rect(x0 + s(24), r.top, tw + s(4), h), t, self.fonts.bold, fg, DT_ROW);
                    }
                    None => text(dc, r, t, self.fonts.bold, fg, DT_MID),
                }
            }
            Kind::Edit { .. } => {
                // DESIGN.md 4.11: field with a stroke; a 2 px accent bottom edge while focused.
                round(dc, r, s(4), Some(FIELD), Some((LINE, s(1))));
                if self.edit_focused(wgt.id) {
                    clip_round(dc, r, s(4), || fill(dc, &rect(r.left, r.bottom - s(2), r.right - r.left, s(2)), ACCENT));
                } else {
                    clip_round(dc, r, s(4), || fill(dc, &rect(r.left, r.bottom - 1, r.right - r.left, 1), TRACK));
                }
            }
            Kind::Radio { on, text: t } => {
                let cy = (r.top + r.bottom) / 2;
                let cx = r.left + s(9);
                if *on {
                    circle(dc, cx, cy, s(9), Some(ACCENT), None);
                    circle(dc, cx, cy, s(4), Some(ON_ACCENT), None);
                } else {
                    circle(dc, cx, cy, s(9), Some(if hot { FIELD_HI } else { FIELD }), Some((if hot { MUTED } else { TRACK }, s(1))));
                }
                text(dc, &rect(r.left + s(28), r.top, r.right - r.left - s(28), r.bottom - r.top), t, if *on { self.fonts.bold } else { self.fonts.body }, TEXT, DT_ROW);
            }
            Kind::Row { title, sub, selected } => {
                // A selectable list row: subtle fill on hover, fill plus the 3 px accent indicator when selected.
                let radius = s(4);
                if *selected {
                    round(dc, r, radius, Some(CARD_HI), Some((LINE, s(1))));
                    clip_round(dc, r, radius, || fill(dc, &rect(r.left, r.top + s(10), s(3), r.bottom - r.top - s(20)), ACCENT));
                } else if hot {
                    round(dc, r, radius, Some(CARD_HI), None);
                }
                text(dc, &rect(r.left + s(14), r.top + s(6), r.right - r.left - s(28), s(20)), title, self.fonts.bold, TEXT, DT_ROW);
                text(dc, &rect(r.left + s(14), r.top + s(25), r.right - r.left - s(28), s(16)), sub, self.fonts.small, MUTED, DT_ROW);
            }
            Kind::Battery => self.draw_big_battery(dc, r),
            Kind::MouseArt { highlight, regions } => pages::draw_mouse(self, dc, r, *highlight, regions),
            Kind::Nav { label, icon: ic, selected } => {
                // DESIGN.md 4.1: white 6 % fill, a 3 x 16 accent indicator on the left when selected.
                let radius = s(4);
                if *selected || hot {
                    clip_round(dc, r, radius, || alpha_rect(dc, r, WHITE, if *selected && hot { 20 } else { 15 }));
                }
                if *selected {
                    let cy = (r.top + r.bottom) / 2;
                    round(dc, &rect(r.left, cy - s(8), s(3), s(16)), s(1), Some(ACCENT), None);
                }
                let color = if *selected || hot { TEXT } else { MUTED };
                icon(dc, self.fonts.icon, r.left + s(20), (r.top + r.bottom) / 2, *ic, color);
                text(dc, &rect(r.left + s(40), r.top, r.right - r.left - s(44), r.bottom - r.top), label, if *selected { self.fonts.bold } else { self.fonts.body }, color, DT_ROW);
            }
            Kind::Tile { title, sub, badge, art, selected } => self.draw_tile(dc, r, title, sub, badge, art, *selected, hot),
            Kind::Badge { text: t, color } => {
                round(dc, r, s(4), Some(mix(*color, CARD, 0.16)), None);
                text(dc, r, t, self.fonts.caps, *color, DT_MID);
            }
            Kind::Bullet { color } => {
                let cy = (r.top + r.bottom) / 2;
                circle(dc, r.left + s(4), cy, s(4), Some(*color), None);
            }
            Kind::Curve { points, samples, editable, selected } => self.draw_curve(dc, r, points, samples, *editable, *selected, hot),
        }
    }

    /// A game tile: cover art (or a generated gradient with the program's icon),
    /// a launcher badge, and the name over a dark fade at the bottom.
    #[allow(clippy::too_many_arguments)]
    unsafe fn draw_tile(&self, dc: HDC, r: &RECT, title: &str, sub: &str, badge: &str, art: &TileArt, selected: bool, hot: bool) {
        let s = |v: i32| self.s(v);
        let radius = s(4);
        if *art == TileArt::Add {
            // DESIGN.md 5.4: the dashed "add a program" tile.
            if hot {
                round(dc, r, radius, Some(CARD_HI), None);
            }
            dashed_round(dc, r, radius, if hot { MUTED } else { TRACK });
            let cy = (r.top + r.bottom) / 2;
            circle(dc, (r.left + r.right) / 2, cy - s(16), s(18), Some(FIELD), Some((TRACK, s(1))));
            icon(dc, self.fonts.icon, (r.left + r.right) / 2, cy - s(16), Icon::Add, TEXT);
            text(dc, &rect(r.left + s(8), cy + s(10), r.right - r.left - s(16), s(20)), title, self.fonts.bold, TEXT, DT_MID);
            text(dc, &rect(r.left + s(8), cy + s(30), r.right - r.left - s(16), s(16)), sub, self.fonts.small, MUTED, DT_MID);
            return;
        }
        // Hover lifts the tile by 2 px with a soft shadow (DESIGN.md 4.12).
        let lifted = offset(r, 0, if hot && !selected { -s(2) } else { 0 });
        let r = &lifted;
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        if hot {
            shadow(dc, r, radius, 4);
        }
        let cover = match art {
            TileArt::Lib(j) => self.covers.get(*j).and_then(|c| c.as_ref()),
            _ => None,
        };
        let hicon = match art {
            TileArt::Lib(j) => self.lib_icons.get(*j).and_then(|i| i.as_ref()).map(|i| i.hicon),
            TileArt::Exe(p) => self.exe_icons.get(p).and_then(|i| i.as_ref()).map(|i| i.hicon),
            TileArt::None | TileArt::Add => None,
        };
        clip_round(dc, r, radius, || {
            match cover {
                Some(c) => draw_cover(dc, r, c.hbm, c.w, c.h),
                None => {
                    // A colour pair picked by the name, so the same game always looks the same.
                    const PALETTE: [(u32, u32); 8] = [
                        (rgb(0x2f, 0x40, 0x72), rgb(0x16, 0x1b, 0x36)),
                        (rgb(0x5e, 0x2c, 0x70), rgb(0x27, 0x12, 0x33)),
                        (rgb(0x1f, 0x60, 0x4c), rgb(0x0f, 0x2c, 0x25)),
                        (rgb(0x74, 0x3c, 0x2a), rgb(0x33, 0x17, 0x0e)),
                        (rgb(0x2a, 0x5c, 0x7c), rgb(0x10, 0x27, 0x38)),
                        (rgb(0x62, 0x5c, 0x1f), rgb(0x2c, 0x29, 0x0e)),
                        (rgb(0x70, 0x2c, 0x3c), rgb(0x33, 0x13, 0x1e)),
                        (rgb(0x3d, 0x3f, 0x44), rgb(0x1c, 0x1d, 0x20)),
                    ];
                    let hash = title.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
                    let (top, bot) = PALETTE[(hash % PALETTE.len() as u32) as usize];
                    vgradient(dc, r, top, bot);
                    glow(dc, r.left + w / 2, r.top + h * 38 / 100, w * 55 / 100, mix(top, WHITE, 0.6), 40);
                    match hicon {
                        Some(ic) => {
                            let sz = s(64);
                            draw_hicon(dc, r.left + (w - sz) / 2, r.top + h * 36 / 100 - sz / 2, sz, ic);
                        }
                        None => {
                            // Up to two initials.
                            let initials: String = title.split_whitespace().filter_map(|p| p.chars().next()).filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_uppercase();
                            text(dc, &rect(r.left, r.top + h * 36 / 100 - s(24), w, s(48)), &initials, self.fonts.big, mix(WHITE, top, 0.85), DT_MID);
                        }
                    }
                }
            }
            // Name over a fade at the bottom; the launcher only on hover (no badge at rest).
            let fade = h * 46 / 100;
            alpha_vgradient(dc, &rect(r.left, r.bottom - fade, w, fade), BLACK, 0, 215);
            let sub_h = if sub.is_empty() { 0 } else { s(16) };
            text(dc, &rect(r.left + s(10), r.bottom - s(26) - sub_h, w - s(20), s(20)), title, self.fonts.bold, TEXT, DT_ROW);
            if sub_h > 0 {
                text(dc, &rect(r.left + s(10), r.bottom - s(24), w - s(20), s(16)), sub, self.fonts.small, MUTED, DT_ROW);
            }
            if hot && !badge.is_empty() {
                let bw = text_width(dc, self.fonts.caps, badge) + s(22);
                let br = rect(r.right - s(8) - bw, r.top + s(8), bw, s(20));
                round(dc, &br, s(4), Some(mix(BLACK, CARD, 0.5)), None);
                circle(dc, br.left + s(9), br.top + s(10), s(3), Some(badge_color(badge)), None);
                text(dc, &rect(br.left + s(16), br.top, bw - s(16), s(20)), badge, self.fonts.caps, TEXT, DT_ROW);
            }
            if hot && !selected {
                alpha_rect(dc, r, WHITE, 12);
            }
        });
        if selected {
            round(dc, r, radius, None, Some((ACCENT, s(2))));
            circle(dc, r.right - s(16), r.top + s(16), s(10), Some(ACCENT), None);
            icon(dc, self.fonts.icon_sm, r.right - s(16), r.top + s(16), Icon::Check, ON_ACCENT);
        } else {
            round(dc, r, radius, None, Some((if hot { MUTED } else { LINE }, s(1))));
        }
    }

    // ------------------------------------------------------------------
    // Dynamic Sensitivity plot
    // ------------------------------------------------------------------

    /// The plotting area inside a curve widget (room for the axis labels).
    fn curve_plot(&self, r: &RECT) -> RECT {
        RECT { left: r.left + self.s(44), top: r.top + self.s(12), right: r.right - self.s(16), bottom: r.bottom - self.s(26) }
    }

    fn curve_px(&self, plot: &RECT, speed: f32, mult: f32) -> (i32, i32) {
        let w = (plot.right - plot.left) as f32;
        let h = (plot.bottom - plot.top) as f32;
        let x = plot.left + (w * speed / accel::MAX_SPEED as f32).round() as i32;
        let y = plot.bottom - (h * mult / accel::MAX_MULTIPLIER).round() as i32;
        (x, y)
    }

    fn curve_from_px(&self, plot: &RECT, x: i32, y: i32) -> (u8, f32) {
        let w = (plot.right - plot.left).max(1) as f32;
        let h = (plot.bottom - plot.top).max(1) as f32;
        let speed = ((x - plot.left) as f32 / w * accel::MAX_SPEED as f32).round().clamp(0.0, accel::MAX_SPEED as f32) as u8;
        let mult = ((plot.bottom - y) as f32 / h * accel::MAX_MULTIPLIER * 100.0).round() / 100.0;
        (speed, mult.clamp(accel::MIN_MULTIPLIER, accel::MAX_MULTIPLIER))
    }

    /// Control point under a client point of a curve widget, if any.
    fn curve_point_at(&self, wgt: &Widget, x: i32, y: i32) -> Option<usize> {
        let Kind::Curve { points, .. } = &wgt.kind else { return None };
        let plot = self.curve_plot(&wgt.rect);
        let r2 = self.s(10) * self.s(10);
        points.iter().enumerate().rev().find(|(_, (sx, sy))| {
            let (px, py) = self.curve_px(&plot, *sx as f32, *sy);
            (px - x) * (px - x) + (py - y) * (py - y) <= r2
        }).map(|(i, _)| i)
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn draw_curve(&self, dc: HDC, r: &RECT, points: &[(u8, f32)], samples: &[f32], editable: bool, selected: Option<usize>, hot: bool) {
        let s = |v: i32| self.s(v);
        let plot = self.curve_plot(r);
        round(dc, r, s(6), Some(FIELD), Some((LINE, s(1))));
        // Grid: speed every 15, multiplier every 0.25, like Synapse's custom editor.
        for i in 0..=7 {
            let (x, _) = self.curve_px(&plot, i as f32 * 15.0, 0.0);
            fill(dc, &rect(x, plot.top, s(1), plot.bottom - plot.top), LINE);
            let label = format!("{}", i * 15);
            let tw = text_width(dc, self.fonts.small, &label);
            text(dc, &rect(x - tw / 2 - s(4), plot.bottom + s(4), tw + s(8), s(18)), &label, self.fonts.small, MUTED, DT_CENTER_ | DT_VCENTER_ | DT_SINGLELINE_ | DT_NOPREFIX_);
        }
        for i in 0..=6 {
            let m = i as f32 * 0.25;
            let (_, y) = self.curve_px(&plot, 0.0, m);
            fill(dc, &rect(plot.left, y, plot.right - plot.left, s(1)), LINE);
            let label = format!("{m:.2}");
            text(dc, &rect(r.left + s(6), y - s(9), s(36), s(18)), &label, self.fonts.small, MUTED, DT_ROW);
        }
        // Axis names.
        text(dc, &rect(plot.right - s(60), plot.top + s(2), s(58), s(16)), "speed", self.fonts.small, DIM, DT_ROW);
        // The curve: sample i is speed i+1; the point at speed 0 is the first control point.
        let color = if editable { ACCENT } else { BLUE };
        let mut pts: Vec<POINT> = Vec::with_capacity(accel::MAX_SPEED as usize + 1);
        if let Some(p0) = points.first() {
            let (x, y) = self.curve_px(&plot, 0.0, p0.1);
            pts.push(POINT { x, y });
        }
        for (i, v) in samples.iter().enumerate().take(accel::MAX_SPEED as usize) {
            let (x, y) = self.curve_px(&plot, (i + 1) as f32, *v);
            pts.push(POINT { x, y });
        }
        if pts.len() >= 2 {
            polyline(dc, &pts, color, s(2));
        }
        // Control points.
        let hot_pt = if hot && editable {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            ScreenToClient(self.hwnd, &mut pt);
            let probe = Widget { id: Id::Curve, rect: *r, kind: Kind::Curve { points: points.to_vec(), samples: Vec::new(), editable, selected }, fixed: false };
            self.curve_point_at(&probe, pt.x, pt.y)
        } else {
            None
        };
        for (i, (sx, sy)) in points.iter().enumerate() {
            let (x, y) = self.curve_px(&plot, *sx as f32, *sy);
            let active = selected == Some(i) || hot_pt == Some(i);
            let rad = if active { s(6) } else { s(4) };
            circle(dc, x, y, rad, Some(if editable { if active { ACCENT_HI } else { ACCENT } } else { BLUE }), Some((BG, s(2))));
            if editable && selected == Some(i) {
                let label = format!("{sx}, x{sy:.2}");
                let tw = text_width(dc, self.fonts.small, &label);
                let lx = (x - tw / 2).clamp(plot.left, plot.right - tw);
                text(dc, &rect(lx, (y - s(26)).max(plot.top), tw + s(4), s(18)), &label, self.fonts.small, TEXT, DT_ROW);
            }
        }
    }

    unsafe fn draw_big_battery(&self, dc: HDC, r: &RECT) {
        let s = |v: i32| self.s(v);
        let bw = s(120);
        let bh = s(52);
        let x = r.left;
        let y = r.top + (r.bottom - r.top - bh) / 2;
        let color = match self.battery {
            Some(b) if b.charging => BLUE,
            Some(b) if b.percent <= 15 => ERR,
            Some(b) if b.percent <= 35 => WARN,
            Some(_) => ACCENT,
            None => DIM,
        };
        let shell = rect(x, y, bw, bh);
        glass(dc, &shell, s(8), FIELD_HI, FIELD, TRACK, 0);
        round(dc, &rect(x + bw + s(2), y + bh / 2 - s(10), s(6), s(20)), s(2), Some(TRACK), None);
        if let Some(b) = self.battery {
            let inner = inset(&shell, s(6), s(6));
            let fw = ((inner.right - inner.left) * b.percent as i32 / 100).max(if b.percent > 0 { s(6) } else { 0 });
            let fr = rect(inner.left, inner.top, fw, inner.bottom - inner.top);
            clip_round(dc, &fr, s(4), || {
                vgradient(dc, &fr, mix(color, WHITE, 0.75), mix(color, BLACK, 0.8));
                alpha_vgradient(dc, &rect(fr.left, fr.top, fw, (fr.bottom - fr.top) / 2), WHITE, 70, 0);
            });
            glow(dc, fr.right, (fr.top + fr.bottom) / 2, s(26), color, 50);
        }
        // The battery cell is the one surface that keeps a reflection (DESIGN.md 3.4).
        reflection(dc, &shell, 24);
        let tx = x + bw + s(28);
        let (big, small) = match self.battery {
            Some(b) => (format!("{}%", b.percent), if b.charging { "Charging".to_string() } else { "On battery".to_string() }),
            None if self.asleep => ("--".to_string(), "Mouse asleep: move it to refresh".to_string()),
            None => ("--".to_string(), "Mouse not connected".to_string()),
        };
        text(dc, &rect(tx, y - s(8), r.right - tx, s(48)), &big, self.fonts.big, TEXT, DT_ROW);
        text(dc, &rect(tx, y + s(36), r.right - tx, s(20)), &small, self.fonts.body, MUTED, DT_ROW);
    }

    // ------------------------------------------------------------------
    // Hit testing and interaction
    // ------------------------------------------------------------------

    fn cursor_x_in(&self, wgt: &Widget) -> i32 {
        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut pt);
            ScreenToClient(self.hwnd, &mut pt);
        }
        pt.x - wgt.rect.left
    }

    fn chip_at(&self, wgt: &Widget, x_rel: i32) -> Option<usize> {
        let xs = match &wgt.kind {
            Kind::Chips { xs, .. } => xs,
            _ => return None,
        };
        xs.iter().position(|(a, b)| x_rel >= *a && x_rel < *b)
    }

    fn track_span(&self, wgt: &Widget, r: &RECT) -> (i32, i32) {
        let show = matches!(wgt.kind, Kind::Slider { show_value: true, .. });
        let right_pad = if show { self.s(80) } else { self.s(10) };
        (r.left + self.s(10), (r.right - right_pad).max(r.left + self.s(40)))
    }

    fn slider_value_at(&self, wgt: &Widget, x: i32) -> i32 {
        let Kind::Slider { min, max, step, .. } = &wgt.kind else { return 0 };
        let (tl, tr) = self.track_span(wgt, &wgt.rect);
        let frac = ((x - tl) as f32 / (tr - tl).max(1) as f32).clamp(0.0, 1.0);
        let raw = *min as f32 + frac * (*max - *min) as f32;
        let stepped = ((raw / *step as f32).round() * *step as f32) as i32;
        stepped.clamp(*min, *max)
    }

    /// Widget under a client-area point; returns the index and the point in widget space.
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        let top = self.content_top();
        let in_view = y >= top && y < top + self.view_h();
        for (i, wgt) in self.widgets.iter().enumerate().rev() {
            if !wgt.interactive() {
                continue;
            }
            let (px, py) = if wgt.fixed { (x, y) } else if in_view { (x, y - top + self.scroll) } else { continue };
            if contains(&wgt.rect, px, py) {
                return Some(i);
            }
        }
        None
    }

    unsafe fn mouse_down(&mut self, x: i32, y: i32) {
        let Some(i) = self.hit(x, y) else {
            trace(&format!("down ({x},{y}) hit nothing"));
            return;
        };
        enum Act {
            Toggle(bool),
            Chip(Option<usize>),
            Slider(i32),
            Button,
            Click,
            Art(Option<usize>),
            Point(Option<usize>),
        }
        let id = self.widgets[i].id;
        let wgt = &self.widgets[i];
        let py = y - self.content_top() + self.scroll;
        let act = match &wgt.kind {
            Kind::Toggle { on } => Act::Toggle(!*on),
            Kind::Chips { .. } => Act::Chip(self.chip_at(wgt, x - wgt.rect.left)),
            Kind::Slider { .. } => Act::Slider(self.slider_value_at(wgt, x)),
            Kind::Button { .. } => Act::Button,
            Kind::Radio { .. } | Kind::Row { .. } | Kind::Nav { .. } | Kind::Tile { .. } => Act::Click,
            Kind::MouseArt { regions, .. } => Act::Art(regions.iter().find(|(_, r)| contains(r, x, py)).map(|(b, _)| *b)),
            Kind::Curve { editable: true, .. } => Act::Point(self.curve_point_at(wgt, x, py)),
            _ => return,
        };
        match act {
            Act::Point(p) => {
                trace(&format!("down ({x},{y}) py={py} widget {i} point {p:?}"));
                self.curve_sel = p;
                if let Some(p) = p {
                    self.drag_pt = Some((i, p));
                    SetCapture(self.hwnd);
                }
                if let Kind::Curve { selected, .. } = &mut self.widgets[i].kind {
                    *selected = p;
                }
                InvalidateRect(self.hwnd, ptr::null(), 0);
            }
            Act::Toggle(v) => self.changed(id, v as i32),
            Act::Chip(Some(k)) => match id {
                Id::Tab(_) => self.changed(Id::Tab(k), 0),
                _ => self.changed(id, k as i32),
            },
            Act::Chip(None) => {}
            Act::Slider(v) => {
                self.drag = Some(i);
                SetCapture(self.hwnd);
                if let Kind::Slider { value, .. } = &mut self.widgets[i].kind {
                    *value = v;
                }
                InvalidateRect(self.hwnd, ptr::null(), 0);
            }
            Act::Button => {
                self.pressed = Some(i);
                InvalidateRect(self.hwnd, ptr::null(), 0);
            }
            Act::Click => self.changed(id, 0),
            Act::Art(Some(b)) => self.changed(Id::SelectButton(b), 0),
            Act::Art(None) => {}
        }
    }

    unsafe fn mouse_move(&mut self, x: i32, y: i32) {
        if !self.tracking_leave {
            let mut tme = TRACKMOUSEEVENT { cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
            TrackMouseEvent(&mut tme);
            self.tracking_leave = true;
        }
        if let Some(i) = self.drag {
            let v = self.slider_value_at(&self.widgets[i], x);
            if let Kind::Slider { value, .. } = &mut self.widgets[i].kind {
                if *value != v {
                    *value = v;
                    InvalidateRect(self.hwnd, ptr::null(), 0);
                }
            }
            return;
        }
        if let Some((i, p)) = self.drag_pt {
            let plot = self.curve_plot(&self.widgets[i].rect);
            let py = y - self.content_top() + self.scroll;
            let (speed, mult) = self.curve_from_px(&plot, x, py);
            trace(&format!("move ({x},{y}) py={py} -> point {p} = ({speed}, {mult})"));
            if let Kind::Curve { points, samples, .. } = &mut self.widgets[i].kind {
                // Keep the order: the first point stays at 0, the others between their neighbours.
                let lo = if p == 0 { 0 } else { points[p - 1].0.saturating_add(1).max(2) };
                let hi = if p + 1 < points.len() { points[p + 1].0.saturating_sub(1) } else { accel::MAX_SPEED };
                let new = (if p == 0 { 0 } else { speed.clamp(lo.min(hi), hi) }, mult);
                if points[p] != new {
                    points[p] = new;
                    *samples = accel::curve(points).to_vec();
                    InvalidateRect(self.hwnd, ptr::null(), 0);
                }
            }
            return;
        }
        let h = self.hit(x, y);
        let chips = h.is_some_and(|i| matches!(self.widgets[i].kind, Kind::Chips { .. }));
        if h != self.hot || chips {
            // Repaint only the widgets whose hover state changed (DESIGN.md 3.6).
            let old = self.hot;
            self.hot = h;
            for i in [old, h].into_iter().flatten() {
                let r = self.widget_rect(i);
                InvalidateRect(self.hwnd, &r, 0);
            }
        }
    }

    /// A widget's rectangle in client coordinates, with room for its hover lift.
    fn widget_rect(&self, i: usize) -> RECT {
        let w = &self.widgets[i];
        let r = if w.fixed { w.rect } else { offset(&w.rect, 0, self.content_top() - self.scroll) };
        inset(&r, -self.s(6), -self.s(6))
    }

    unsafe fn mouse_up(&mut self, x: i32, y: i32) {
        if let Some(i) = self.drag.take() {
            ReleaseCapture();
            let v = self.slider_value_at(&self.widgets[i], x);
            self.changed(self.widgets[i].id, v);
            return;
        }
        if let Some((i, _)) = self.drag_pt.take() {
            ReleaseCapture();
            trace(&format!("up ({x},{y}) widget {i}"));
            if let Kind::Curve { points, .. } = &self.widgets[i].kind {
                if *points != self.cfg.acceleration.curve {
                    self.cfg.acceleration.curve = points.clone();
                    self.commit(Write::Accel);
                }
            }
            return;
        }
        if let Some(i) = self.pressed.take() {
            if self.hit(x, y) == Some(i) {
                self.changed(self.widgets[i].id, 0);
            } else {
                InvalidateRect(self.hwnd, ptr::null(), 0);
            }
        }
    }

    unsafe fn edit_committed(&mut self, h: HWND) {
        let Some(e) = self.edits.iter().find(|e| e.hwnd == h) else { return };
        let id = e.id;
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        let text = String::from_utf16_lossy(&buf[..n as usize]).trim().to_string();
        match id {
            Id::StageValue(i) => {
                if let Ok(v) = text.parse::<i32>() {
                    let v = clamp_dpi(v, &self.spec);
                    if self.cfg.dpi.stages.get(i).copied() != Some(v as u16) {
                        self.changed(id, v);
                        return;
                    }
                }
            }
            Id::StageY(i) => {
                if let Ok(v) = text.parse::<i32>() {
                    let v = clamp_dpi(v, &self.spec);
                    if self.cfg.dpi.y_stages.get(i).copied() != Some(v as u16) {
                        self.changed(id, v);
                        return;
                    }
                }
            }
            Id::GamesEdit => {
                let games: Vec<String> = text.split([',', ';']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                if games != self.cfg.polling.games {
                    self.cfg.polling.games = games;
                    self.commit(Write::None);
                    return;
                }
            }
            Id::KeyEdit => {
                if text.is_empty() {
                    return;
                }
                let key = self.button_key();
                // Keep a turbo rate that was already set on this button.
                let rate = self.layer_buttons().get(&key).map(|s| pages::turbo_split(s).1).unwrap_or(None);
                let spec = pages::with_turbo(&format!("key:{}", text.to_lowercase()), rate);
                match parse_button(&spec) {
                    Ok(Some(_)) => {
                        self.key_mode = None;
                        if self.layer_buttons().get(&key) != Some(&spec) {
                            self.layer_buttons_mut().insert(key, spec);
                            let w = self.button_write();
                            self.commit(w);
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => self.set_status(&e, ERR),
                }
            }
            Id::TextEdit => {
                if text.is_empty() {
                    return;
                }
                // A one-step macro that the tray app types with the button.
                let key = self.button_key();
                let step = format!("text:{text}");
                let label = if text.chars().count() > 24 { format!("{}...", text.chars().take(22).collect::<String>()) } else { text.clone() };
                let id = self.cfg.ensure_step_macro(&step, &label);
                let spec = macro_spec(id, Playback::Times(1));
                self.text_mode = None;
                if self.layer_buttons().get(&key) != Some(&spec) {
                    self.layer_buttons_mut().insert(key, spec);
                    let w = self.button_write();
                    self.commit(w);
                    return;
                }
            }
            Id::MacroName => {
                if let Some(m) = self.sel_macro.and_then(|i| self.cfg.macros.get_mut(i)) {
                    let name = text.chars().take(60).collect::<String>();
                    if !name.is_empty() && name != m.name {
                        m.name = name;
                        let id = m.id;
                        self.commit(Write::Macro(id));
                        return;
                    }
                }
            }
            Id::GameExe => {
                if let Some(g) = self.cfg.games.get_mut(self.sel_game) {
                    // A game found in a launcher is matched by its folder, so its program name may be empty.
                    if (!text.is_empty() || !g.dir.is_empty()) && text != g.exe {
                        g.exe = text;
                        self.commit(Write::None);
                        return;
                    }
                }
            }
            Id::GameName => {
                if let Some(g) = self.cfg.games.get_mut(self.sel_game) {
                    let name = text.chars().take(80).collect::<String>();
                    if name != g.name {
                        g.name = name;
                        self.commit(Write::None);
                        return;
                    }
                }
            }
            _ => {}
        }
        self.layout();
        InvalidateRect(self.hwnd, ptr::null(), 0);
    }

    /// Live update while typing (the game list's search box).
    unsafe fn edit_changed(&mut self, h: HWND) {
        let Some(e) = self.edits.iter().find(|e| e.hwnd == h) else { return };
        if e.id != Id::GameFilter {
            return;
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        let text = String::from_utf16_lossy(&buf[..n as usize]);
        if text != self.game_filter {
            self.game_filter = text;
            self.layout();
            InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    // ------------------------------------------------------------------
    // Games: launcher scan, the combined list, adding a program by hand
    // ------------------------------------------------------------------

    /// Look for installed games on a background thread; the result arrives as WM_LIBRARY.
    unsafe fn start_scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let hwnd = self.hwnd;
        std::thread::spawn(move || {
            let scanned: Scanned = library::scan()
                .into_iter()
                .map(|g| {
                    let cover = g.art.as_deref().and_then(|p| art::load_cover(p, COVER_W, COVER_H, CARD));
                    let icon = if cover.is_none() {
                        let exe = if g.exe.is_empty() { art::guess_exe(&g.dir) } else { Some(format!("{}\\{}", g.dir.trim_end_matches('\\'), g.exe)) };
                        exe.and_then(|p| art::exe_icon(&p, ICON_PX))
                    } else {
                        None
                    };
                    (g, cover, icon)
                })
                .collect();
            let games = Box::into_raw(Box::new(scanned));
            if unsafe { PostMessageW(hwnd, WM_LIBRARY, 0, games as LPARAM) } == 0 {
                drop(unsafe { Box::from_raw(games) });
            }
        });
    }

    unsafe fn library_loaded(&mut self, games: Scanned) {
        let manual = !self.library.is_empty();
        self.library.clear();
        self.covers.clear();
        self.lib_icons.clear();
        for (g, c, i) in games {
            self.library.push(g);
            self.covers.push(c);
            self.lib_icons.push(i);
        }
        self.scanning = false;
        if manual {
            let n = self.library.len();
            self.set_status(&format!("Found {n} installed game{}", if n == 1 { "" } else { "s" }), ACCENT);
        }
        self.layout();
        InvalidateRect(self.hwnd, ptr::null(), 0);
    }

    /// Launcher a profile's game was found in, or `None` for a program added by hand.
    pub(crate) fn game_source(&self, g: &GameConfig) -> Option<&'static str> {
        self.library.iter().find(|l| same_game(g, l)).map(|l| l.source)
    }

    /// The launchers games were found in, with how many each, in order of first appearance.
    pub(crate) fn library_sources(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for l in &self.library {
            if !out.contains(&l.source) {
                out.push(l.source);
            }
        }
        out
    }

    /// Picture for a profile's tile: its installed game's cover, else its program's icon.
    pub(crate) fn profile_art(&self, g: &GameConfig) -> TileArt {
        if let Some(j) = self.library.iter().position(|l| same_game(g, l)) {
            return TileArt::Lib(j);
        }
        match self.exe_path_of(g) {
            Some(p) if self.exe_icons.get(&p).is_some_and(|i| i.is_some()) => TileArt::Exe(p),
            _ => TileArt::None,
        }
    }

    /// Full path of a profile's program, when it can be known.
    fn exe_path_of(&self, g: &GameConfig) -> Option<String> {
        if g.exe.contains('\\') {
            return Some(g.exe.clone());
        }
        if g.dir.is_empty() {
            return None;
        }
        if !g.exe.is_empty() {
            return Some(format!("{}\\{}", g.dir.trim_end_matches('\\'), g.exe));
        }
        self.exe_guess.get(&g.dir).cloned().flatten()
    }

    /// Load the icons of the profiles' programs that are not in the library (once per path).
    fn warm_icons(&mut self) {
        let games: Vec<GameConfig> = self.cfg.games.iter().filter(|g| !self.library.iter().any(|l| same_game(g, l))).cloned().collect();
        for g in games {
            if !g.dir.is_empty() && g.exe.is_empty() && !self.exe_guess.contains_key(&g.dir) {
                let guess = art::guess_exe(&g.dir);
                self.exe_guess.insert(g.dir.clone(), guess);
            }
            if let Some(p) = self.exe_path_of(&g) {
                if !self.exe_icons.contains_key(&p) {
                    let ic = art::exe_icon(&p, ICON_PX);
                    self.exe_icons.insert(p, ic);
                }
            }
        }
    }

    /// Installed games that have no profile yet (indices into `library`).
    pub(crate) fn games_without_profile(&self) -> Vec<usize> {
        (0..self.library.len()).filter(|j| !self.cfg.games.iter().any(|g| same_game(g, &self.library[*j]))).collect()
    }

    /// Select a profile just created, open its settings and bring it into view.
    unsafe fn created_game(&mut self) {
        self.sel_game = self.cfg.games.len() - 1;
        self.game_open = true;
        self.scroll_to = Some(Id::GameRow(self.sel_game));
        let name = self.cfg.games[self.sel_game].label();
        let n = self.cfg.games.len();
        self.commit(Write::None);
        if self.cfg.games.len() == n {
            self.set_status(&format!("Profile created for {name}: pick its DPI stage and polling rate"), ACCENT);
        }
    }

    /// Ask for a program with the standard Open dialog and give it a profile.
    unsafe fn browse_program(&mut self) {
        if let Some(path) = self.open_dialog("Choose the game's program", "Programs (*.exe)\0*.exe\0All files (*.*)\0*.*\0\0") {
            self.add_program(&path);
        }
    }

    /// The standard Open dialog; `filter` is the usual NUL-separated pair list. None when cancelled.
    unsafe fn open_dialog(&self, title: &str, filter: &str) -> Option<String> {
        use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        use windows_sys::Win32::UI::Controls::Dialogs::*;
        // The dialog's shell views want COM on this thread; a second call is harmless.
        CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let mut file = [0u16; 1024];
        let filter: Vec<u16> = filter.encode_utf16().collect();
        let title = wide(title);
        let mut ofn: OPENFILENAMEW = mem::zeroed();
        ofn.lStructSize = mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = self.hwnd;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = file.as_mut_ptr();
        ofn.nMaxFile = file.len() as u32;
        ofn.lpstrTitle = title.as_ptr();
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_HIDEREADONLY | OFN_DONTADDTORECENT;
        if GetOpenFileNameW(&mut ofn) == 0 {
            return None;
        }
        let n = file.iter().position(|c| *c == 0).unwrap_or(file.len());
        Some(String::from_utf16_lossy(&file[..n]))
    }

    /// Profile for a program picked by hand, matched by its file name.
    unsafe fn add_program(&mut self, path: &str) {
        let p = std::path::Path::new(path);
        let exe = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        if exe.is_empty() {
            return;
        }
        if let Some(i) = self.cfg.games.iter().position(|g| g.exe.eq_ignore_ascii_case(&exe)) {
            self.sel_game = i;
            self.game_open = true;
            self.scroll_to = Some(Id::GameRow(i));
            self.set_status(&format!("{exe} already has a profile"), WARN);
            self.layout();
            InvalidateRect(self.hwnd, ptr::null(), 0);
            return;
        }
        // Inside a game found in a launcher: use that game's name.
        let full = library::norm_dir(path);
        let name = self
            .library
            .iter()
            .find(|l| library::is_inside(&full, &library::norm_dir(&l.dir)))
            .map(|l| l.name.clone())
            .unwrap_or_else(|| p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        self.cfg.games.push(GameConfig { name, exe, ..Default::default() });
        self.created_game();
    }

    // ------------------------------------------------------------------
    // Changes
    // ------------------------------------------------------------------

    pub(crate) fn button_sel(&self) -> (u8, bool) {
        (BUTTONS[self.sel_button].1, self.layer == Layer::Hypershift)
    }

    /// Config key of the selected button in the selected layer.
    pub(crate) fn button_key(&self) -> String {
        let name = BUTTONS[self.sel_button].0;
        if self.layer == Layer::Hypershift { format!("hypershift.{name}") } else { name.to_string() }
    }

    /// The button map the selected layer lives in (`[buttons]` or a game's).
    pub(crate) fn layer_buttons(&self) -> &std::collections::BTreeMap<String, String> {
        match self.layer {
            Layer::Game(i) if i < self.cfg.games.len() => &self.cfg.games[i].buttons,
            _ => &self.cfg.buttons,
        }
    }

    fn layer_buttons_mut(&mut self) -> &mut std::collections::BTreeMap<String, String> {
        match self.layer {
            Layer::Game(i) if i < self.cfg.games.len() => &mut self.cfg.games[i].buttons,
            _ => &mut self.cfg.buttons,
        }
    }

    /// What a change to the selected button writes: the onboard function for the
    /// normal and Hypershift layers, nothing for a game layer (the tray applies it).
    fn button_write(&self) -> Write {
        match self.layer {
            Layer::Game(_) => Write::None,
            _ => {
                let (code, hs) = self.button_sel();
                Write::Button(code, hs)
            }
        }
    }

    // ------------------------------------------------------------------
    // Macro recording
    // ------------------------------------------------------------------

    unsafe fn start_recording(&mut self, id: u16) {
        self.recording = Some((record::Recorder::new(), id));
        self.rec_count = 0;
        self.rec_secs = 0;
        SetTimer(self.hwnd, TIMER_RECORD, 10, None);
        self.set_status("Recording: press keys and mouse buttons, then click Stop or press Esc", WARN);
        self.layout();
        InvalidateRect(self.hwnd, ptr::null(), 0);
    }

    unsafe fn record_tick(&mut self) {
        let hwnd = self.hwnd;
        let Some((rec, _)) = self.recording.as_mut() else {
            KillTimer(hwnd, TIMER_RECORD);
            return;
        };
        rec.poll(hwnd);
        let (n, secs) = (rec.events.len(), rec.elapsed_secs());
        if n != self.rec_count || secs != self.rec_secs {
            self.rec_count = n;
            self.rec_secs = secs;
            self.layout();
            InvalidateRect(hwnd, ptr::null(), 0);
        }
    }

    unsafe fn stop_recording(&mut self) {
        KillTimer(self.hwnd, TIMER_RECORD);
        let Some((rec, id)) = self.recording.take() else { return };
        let events = rec.finish();
        if events.is_empty() {
            self.set_status("Nothing recorded; the macro is unchanged", WARN);
            self.layout();
            InvalidateRect(self.hwnd, ptr::null(), 0);
            return;
        }
        let specs = macros::event_specs(&events);
        match self.cfg.macros.iter_mut().find(|m| m.id == id) {
            Some(m) => m.events = specs,
            None => {
                self.cfg.macros.push(MacroConfig { id, name: format!("Macro {id}"), events: specs });
                self.sel_macro = Some(self.cfg.macros.len() - 1);
            }
        }
        self.commit(Write::Macro(id));
    }

    unsafe fn changed(&mut self, id: Id, v: i32) {
        let write = match id {
            Id::None => return,
            Id::Tab(i) => {
                self.page = Page::TABS[i];
                self.scroll = 0;
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::Gear => {
                self.page = Page::App;
                self.scroll = 0;
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::StagesEnabled => {
                let d = &mut self.cfg.dpi;
                let current = d.stages.get(d.active as usize - 1).copied().unwrap_or(800);
                if v == 1 {
                    let mut stages = vec![400u16, 800, 1600, 3200, 6400];
                    if !stages.contains(&current) {
                        stages.push(current);
                        stages.sort_unstable();
                        stages.truncate(5);
                    }
                    d.active = stages.iter().position(|s| *s == current).unwrap_or(0) as u8 + 1;
                    d.stages = stages;
                } else {
                    d.stages = vec![current];
                    d.active = 1;
                }
                Write::Stages
            }
            Id::StageCount => {
                let n = (v as usize + 1).clamp(1, 5);
                let d = &mut self.cfg.dpi;
                while d.stages.len() < n {
                    let last = *d.stages.last().unwrap_or(&800);
                    d.stages.push(clamp_dpi(last.saturating_mul(2) as i32, &self.spec) as u16);
                }
                d.stages.truncate(n);
                d.active = d.active.clamp(1, n as u8);
                Write::Stages
            }
            Id::StageValue(i) => {
                if let Some(s) = self.cfg.dpi.stages.get_mut(i) {
                    *s = clamp_dpi(v, &self.spec) as u16;
                }
                Write::Stages
            }
            Id::StageY(i) => {
                let d = &mut self.cfg.dpi;
                if d.y_stages.len() != d.stages.len() {
                    d.y_stages = d.stages.clone();
                }
                if let Some(s) = d.y_stages.get_mut(i) {
                    *s = clamp_dpi(v, &self.spec) as u16;
                }
                Write::Stages
            }
            Id::XySplit => {
                let d = &mut self.cfg.dpi;
                d.y_stages = if v == 1 { d.stages.clone() } else { Vec::new() };
                Write::Stages
            }
            Id::AccelOn => {
                self.cfg.acceleration.enabled = v == 1;
                Write::Accel
            }
            Id::AccelMode => {
                let modes = [accel::Mode::Classic, accel::Mode::Natural, accel::Mode::Jump, accel::Mode::Custom];
                self.cfg.acceleration.mode = modes[(v as usize).min(3)].name().into();
                self.curve_sel = None;
                Write::Accel
            }
            Id::Curve => Write::Accel,
            Id::CurveAdd => {
                // New point in the middle of the widest gap (or after the last point).
                let c = &mut self.cfg.acceleration.curve;
                if c.len() >= accel::MAX_CONTROL_POINTS {
                    return;
                }
                let last = c.last().map(|p| p.0).unwrap_or(0);
                let (mut at, mut gap) = (c.len(), accel::MAX_SPEED.saturating_sub(last));
                for i in 1..c.len() {
                    let g = c[i].0 - c[i - 1].0;
                    if g > gap {
                        gap = g;
                        at = i;
                    }
                }
                if gap < 2 {
                    return;
                }
                let (x, y) = if at == c.len() {
                    (last + gap / 2, c.last().map(|p| p.1).unwrap_or(1.0))
                } else {
                    (c[at - 1].0 + gap / 2, (c[at - 1].1 + c[at].1) / 2.0)
                };
                c.insert(at, (x, (y * 100.0).round() / 100.0));
                self.curve_sel = Some(at);
                Write::Accel
            }
            Id::CurveRemove => {
                let c = &mut self.cfg.acceleration.curve;
                match self.curve_sel {
                    Some(i) if i > 0 && i < c.len() && c.len() > 2 => {
                        c.remove(i);
                        self.curve_sel = None;
                        Write::Accel
                    }
                    _ => return,
                }
            }
            Id::CurveTemplate(k) => {
                let modes = [accel::Mode::Classic, accel::Mode::Natural, accel::Mode::Jump];
                self.cfg.acceleration.curve = modes[k.min(2)].template();
                self.curve_sel = None;
                Write::Accel
            }
            Id::Turbo => {
                let key = self.button_key();
                let Some(cur) = self.layer_buttons().get(&key).cloned() else { return };
                let (base, _) = pages::turbo_split(&cur);
                let spec = pages::with_turbo(&base, if v == 1 { Some(10) } else { None });
                self.layer_buttons_mut().insert(key, spec);
                self.button_write()
            }
            Id::TurboRate => {
                let key = self.button_key();
                let Some(cur) = self.layer_buttons().get(&key).cloned() else { return };
                let (base, _) = pages::turbo_split(&cur);
                self.layer_buttons_mut().insert(key, pages::with_turbo(&base, Some(v as u8)));
                self.button_write()
            }
            Id::MacroPlay | Id::MacroTimes => {
                let key = self.button_key();
                let Some(cur) = self.layer_buttons().get(&key).cloned() else { return };
                let Some((mid, play)) = parse_button(&cur).ok().flatten().and_then(|f| f.macro_binding()) else { return };
                let play = if id == Id::MacroTimes {
                    Playback::Times(v.clamp(1, macros::TIMES_MAX as i32) as u8)
                } else {
                    match v {
                        0 => Playback::Times(1),
                        1 => Playback::Times(if let Playback::Times(n) = play { n.max(2) } else { 2 }),
                        2 => Playback::Held,
                        3 => Playback::Toggle,
                        _ => Playback::Sequence,
                    }
                };
                self.layer_buttons_mut().insert(key, macro_spec(mid, play));
                self.button_write()
            }
            Id::SelectMacro(i) => {
                self.sel_macro = Some(i.min(self.cfg.macros.len().saturating_sub(1)));
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::MacroNew => {
                let id = self.cfg.next_macro_id();
                self.start_recording(id);
                return;
            }
            Id::MacroRecord => {
                let Some(id) = self.sel_macro.and_then(|i| self.cfg.macros.get(i)).map(|m| m.id) else { return };
                self.start_recording(id);
                return;
            }
            Id::MacroStop => {
                self.stop_recording();
                return;
            }
            Id::MacroDelete => {
                let Some(i) = self.sel_macro.filter(|i| *i < self.cfg.macros.len()) else { return };
                let id = self.cfg.macros[i].id;
                // Buttons bound to it go back to their default.
                let bound: Vec<String> = self.cfg.buttons.iter().filter(|(_, s)| parse_button(s).ok().flatten().and_then(|f| f.macro_binding()).map(|(m, _)| m) == Some(id)).map(|(k, _)| k.clone()).collect();
                for k in &bound {
                    self.cfg.buttons.remove(k);
                }
                for g in &mut self.cfg.games {
                    g.buttons.retain(|_, s| parse_button(s).ok().flatten().and_then(|f| f.macro_binding()).map(|(m, _)| m) != Some(id));
                }
                self.cfg.macros.remove(i);
                self.sel_macro = if self.cfg.macros.is_empty() { None } else { Some(i.min(self.cfg.macros.len() - 1)) };
                if !bound.is_empty() {
                    // The onboard functions of the unbound buttons are rewritten by the delete.
                }
                Write::MacroDelete(id)
            }
            Id::GameRow(i) => {
                // Clicking the open profile closes it.
                self.game_open = !(self.game_open && self.sel_game == i);
                self.sel_game = i.min(self.cfg.games.len().saturating_sub(1));
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::GameLib(j) => {
                let Some(l) = self.library.get(j) else { return };
                // Matched by the install folder: covers the game's launcher and sub-programs too.
                self.cfg.games.push(GameConfig { name: l.name.clone(), dir: l.dir.clone(), ..Default::default() });
                self.created_game();
                return;
            }
            Id::GameScan => {
                self.start_scan();
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::GameSource => {
                self.lib_source = if v <= 0 { None } else { self.library_sources().get(v as usize - 1).copied() };
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::GameBrowse => {
                self.browse_program();
                return;
            }
            Id::GameButtons => {
                if self.sel_game < self.cfg.games.len() {
                    self.layer = Layer::Game(self.sel_game);
                    self.key_mode = None;
                    self.page = Page::Customize;
                    self.scroll = 0;
                }
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::GameRemove => {
                if self.sel_game < self.cfg.games.len() {
                    // A launcher game goes back to the installed games list.
                    let name = self.cfg.games.remove(self.sel_game).label();
                    self.set_status(&format!("Profile of {name} removed"), ACCENT);
                    self.game_open = false;
                    self.sel_game = self.sel_game.min(self.cfg.games.len().saturating_sub(1));
                    if let Layer::Game(_) = self.layer {
                        self.layer = Layer::Normal;
                    }
                }
                Write::None
            }
            Id::GameStage => {
                if let Some(g) = self.cfg.games.get_mut(self.sel_game) {
                    g.dpi_stage = if v == 0 { None } else { Some(v as u8) };
                }
                Write::None
            }
            Id::GamePolling => {
                let rates: &[u16] = self.spec.polling_rates;
                if let Some(g) = self.cfg.games.get_mut(self.sel_game) {
                    g.polling = if v == 0 { None } else { rates.get(v as usize - 1).copied() };
                }
                Write::None
            }
            Id::PairStart => {
                self.confirm_unpair = false;
                Write::Pair(PairAct::Start)
            }
            Id::PairCancel => Write::Pair(PairAct::Cancel),
            Id::Unpair => {
                if !self.confirm_unpair {
                    self.confirm_unpair = true;
                    self.set_status("Unpairing stops the mouse working until it is paired again. Click \"Yes, unpair\" to confirm", WARN);
                    self.layout();
                    InvalidateRect(self.hwnd, ptr::null(), 0);
                    return;
                }
                self.confirm_unpair = false;
                Write::Pair(PairAct::Unpair)
            }
            Id::AutoPair => Write::Pair(PairAct::Auto(v == 1)),
            Id::StageActive(i) => {
                self.cfg.dpi.active = i as u8 + 1;
                Write::Stages
            }
            Id::Polling => {
                let rates: &[u16] = self.spec.polling_rates;
                self.cfg.polling.rate = rates[(v as usize).min(rates.len() - 1)];
                Write::Polling
            }
            Id::GameRate => {
                let rates: &[u16] = self.spec.polling_rates;
                self.cfg.polling.game_rate = rates[(v as usize).min(rates.len() - 1)];
                Write::None
            }
            Id::RotationOn => {
                self.cfg.sensor.rotation = if v == 1 { self.rotation_last } else { 0 };
                Write::Rotation
            }
            Id::Rotation => {
                self.cfg.sensor.rotation = v.clamp(-30, 30) as i8;
                if v != 0 {
                    self.rotation_last = v as i8;
                }
                Write::Rotation
            }
            Id::TrackMode => {
                let mode = ["smart", "keep"][(v as usize).min(1)];
                self.cfg.sensor.tracking = mode.into();
                self.fix_sensor_ranges();
                Write::Tracking
            }
            Id::Asym => {
                self.cfg.sensor.asymmetric = v == 1;
                self.fix_sensor_ranges();
                Write::Tracking
            }
            Id::Lift => {
                self.cfg.sensor.lift = v as u8;
                self.fix_sensor_ranges();
                Write::Tracking
            }
            Id::Land => {
                self.cfg.sensor.land = v as u8;
                self.fix_sensor_ranges();
                Write::Tracking
            }
            Id::Profile => {
                self.cfg.profile.active = (v + 1).clamp(1, 5) as u8;
                Write::Profile
            }
            Id::Idle => {
                self.cfg.power.idle_seconds = (v * 60).clamp(60, 900) as u16;
                Write::Idle
            }
            Id::LowBattery => {
                self.cfg.power.low_battery_percent = v.clamp(5, 100) as u8;
                Write::LowBattery
            }
            Id::DongleLed => {
                self.cfg.dongle.led = (v + 1).clamp(1, 3) as u8;
                Write::DongleLed
            }
            Id::NotifyPercent => {
                self.cfg.tray.notify_percent = v.clamp(5, 50) as u8;
                Write::None
            }
            Id::PollSeconds => {
                self.cfg.tray.poll_seconds = v.clamp(10, 600) as u32;
                Write::None
            }
            Id::Autostart => {
                match autostart::set(v == 1) {
                    Ok(()) => {
                        self.autostart = v == 1;
                        self.set_status(if v == 1 { "The tray app will start at login" } else { "Autostart disabled" }, ACCENT);
                    }
                    Err(e) => self.set_status(&e, ERR),
                }
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::OpenConfig => {
                let path = config_path();
                let _ = Config::load_or_create(&path);
                ShellExecuteW(0, wide("open").as_ptr(), wide(&path.to_string_lossy()).as_ptr(), ptr::null(), ptr::null(), SW_SHOWNORMAL);
                return;
            }
            Id::Reimport => {
                self.reimport();
                return;
            }
            Id::Reapply => {
                self.pending = true;
                self.retry_pending();
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::Refresh => {
                self.read_live(true);
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::SelectButton(i) => {
                self.sel_button = i.min(BUTTONS.len() - 1);
                self.key_mode = None;
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::Layer => {
                self.layer = match v {
                    0 => Layer::Normal,
                    1 => Layer::Hypershift,
                    n => Layer::Game(((n - 2) as usize).min(self.cfg.games.len().saturating_sub(1))),
                };
                self.key_mode = None;
                self.layout();
                InvalidateRect(self.hwnd, ptr::null(), 0);
                return;
            }
            Id::Category(_) => {
                let c = (v as usize).min(pages::CATEGORIES.len() - 1);
                let key = self.button_key();
                self.key_mode = None;
                self.text_mode = None;
                let spec = match pages::CATEGORIES[c] {
                    "Default" => None,
                    "Mouse" => Some(format!("mouse:{}", BUTTONS[self.sel_button].0)),
                    "Keyboard" => {
                        // Show the key box; nothing is written until a key name is typed.
                        if !self.layer_buttons().get(&key).is_some_and(|s| s.starts_with("key:")) {
                            self.key_mode = Some(key);
                        }
                        self.layout();
                        InvalidateRect(self.hwnd, ptr::null(), 0);
                        return;
                    }
                    "DPI" => Some("dpi-up".to_string()),
                    "Multimedia" => Some("media:play-pause".to_string()),
                    "System" => Some("system:sleep".to_string()),
                    "Macro" => {
                        let Some(m) = self.cfg.macros.first() else {
                            self.set_status("No macros yet: record one in the MACROS section below", WARN);
                            self.layout();
                            InvalidateRect(self.hwnd, ptr::null(), 0);
                            return;
                        };
                        Some(macro_spec(m.id, Playback::Times(1)))
                    }
                    "Program" => {
                        // A one-step macro that the tray app plays: pick the program now.
                        let Some(path) = self.open_dialog("Program to start with this button", "Programs (*.exe)\0*.exe\0All files (*.*)\0*.*\0\0") else {
                            self.layout();
                            InvalidateRect(self.hwnd, ptr::null(), 0);
                            return;
                        };
                        let step = format!("launch:{path}");
                        let id = self.cfg.ensure_step_macro(&step, &crate::macros::launch_label(&path));
                        Some(macro_spec(id, Playback::Times(1)))
                    }
                    "Text" => {
                        // Show the text box; nothing is written until a text is typed.
                        self.text_mode = Some(key);
                        self.layout();
                        InvalidateRect(self.hwnd, ptr::null(), 0);
                        return;
                    }
                    "Hypershift" => Some("hypershift".to_string()),
                    _ => Some("disabled".to_string()),
                };
                match spec {
                    Some(s) => {
                        self.layer_buttons_mut().insert(key, s);
                    }
                    None => {
                        self.layer_buttons_mut().remove(&key);
                    }
                }
                self.button_write()
            }
            Id::Option(_) => {
                let key = self.button_key();
                let cur = self.layer_buttons().get(&key).cloned().unwrap_or_default();
                let cat = pages::category_of(Some(&cur), &self.cfg.macros);
                let rate = pages::turbo_split(&cur).1;
                if cat == pages::CAT_MACRO {
                    // Keep the playback option, change the macro.
                    let play = parse_button(&cur).ok().flatten().and_then(|f| f.macro_binding()).map(|(_, p)| p).unwrap_or(Playback::Times(1));
                    if let Some(m) = self.cfg.macros.get(v as usize) {
                        let spec = macro_spec(m.id, play);
                        self.layer_buttons_mut().insert(key, spec);
                    }
                } else {
                    let opts = pages::options_for(cat, self.cfg.dpi.stages.len());
                    if let Some((_, spec)) = opts.get(v as usize) {
                        self.layer_buttons_mut().insert(key, pages::with_turbo(spec, rate));
                    }
                }
                self.button_write()
            }
            Id::KeyEdit | Id::TextEdit | Id::GamesEdit | Id::MacroName | Id::GameExe | Id::GameName | Id::GameFilter => return,
        };
        self.commit(write);
    }

    /// Keep lift/land inside what the selected tracking mode accepts.
    /// Keep lift/land inside what the selected tracking mode accepts; switching
    /// between the two modes starts from Synapse's defaults.
    fn fix_sensor_ranges(&mut self) {
        use crate::sensor::*;
        let s = &mut self.cfg.sensor;
        if s.asymmetric {
            if !(ASYM_LIFT_MIN..=ASYM_LIFT_MAX).contains(&s.lift) {
                s.lift = ASYM_LIFT_DEFAULT;
                s.land = ASYM_LAND_DEFAULT;
            }
            s.lift = s.lift.clamp(ASYM_LIFT_MIN, ASYM_LIFT_MAX);
            s.land = s.land.clamp(ASYM_LAND_MIN, s.lift - 1);
        } else {
            if s.lift > 3 {
                s.lift = 1;
            }
            s.lift = s.lift.clamp(1, 3);
        }
    }

    /// Save the config file and write the changed setting to the mouse. A
    /// change the config rejects (for example losing the last left click) is
    /// undone in the window too.
    unsafe fn commit(&mut self, write: Write) {
        if let Err(e) = self.cfg.validate_for(&self.spec) {
            self.set_status(&format!("Not changed: {e}"), ERR);
            self.cfg = self.last_good.clone();
            self.key_mode = None;
            self.layout();
            InvalidateRect(self.hwnd, ptr::null(), 0);
            return;
        }
        if let Err(e) = Config::write_new(&config_path(), &self.cfg) {
            self.set_status(&e, ERR);
        }
        self.last_good = self.cfg.clone();
        if write != Write::None {
            // The window updates now; the mouse is written in the background (write_done follows).
            self.set_status("Saving to the mouse...", MUTED);
            let cfg = self.cfg.clone();
            self.io(move |dev| {
                let result = dev.mouse().and_then(|m| Self::write_to(m, &cfg, write));
                dev.check(&result);
                // Pairing and macro changes also refresh what the window shows about them.
                let paired = match (write, &result, dev.mouse.as_ref()) {
                    (Write::Pair(_), Ok(()), Some(m)) => Some(m.paired_devices().ok()),
                    _ => None,
                };
                let auto = if paired.is_some() { dev.auto_pairing() } else { None };
                let storage = match (write, dev.mouse.as_ref()) {
                    (Write::Macro(_) | Write::MacroDelete(_), Some(m)) => m.macro_storage().ok(),
                    _ => None,
                };
                Box::new(move |ui: &mut Ui| unsafe { ui.write_done(write, result, paired, auto, storage) })
            });
        } else if self.owner != 0 {
            PostMessageW(self.owner, WM_UI_CONFIG_CHANGED, 1, 0);
        }
        self.layout();
        InvalidateRect(self.hwnd, ptr::null(), 0);
    }

    /// Write one change to the mouse (runs on the I/O thread).
    fn write_to(m: &Mouse, cfg: &Config, write: Write) -> Result<(), HidError> {
        let cfg = cfg.clone();
        let result: Result<(), HidError> = (|| {
            match write {
                Write::None => {}
                Write::Stages => m.set_dpi_stages(cfg.dpi.active, &cfg.stage_pairs())?,
                Write::Polling => m.set_polling_rate(cfg.polling.rate)?,
                Write::Idle => m.set_idle_time(cfg.power.idle_seconds)?,
                Write::LowBattery => m.set_low_battery_threshold(cfg.power.low_battery_percent)?,
                Write::Rotation => m.set_rotation(cfg.sensor.rotation)?,
                Write::DongleLed => {
                    if m.is_wireless() {
                        m.set_dongle_led_mode(cfg.dongle.led)?;
                    }
                }
                Write::Tracking => {
                    if let Ok(Some(t)) = cfg.tracking() {
                        let plan = t.plan().map_err(|_| HidError::Unsupported("invalid tracking"))?;
                        m.apply_tracking(&plan)?;
                    }
                }
                Write::Accel => {
                    let mode = cfg.acceleration.mode().map_err(|_| HidError::Unsupported("invalid acceleration mode"))?;
                    let samples = cfg.acceleration.samples();
                    m.set_acceleration(cfg.acceleration.enabled, mode, Some(&samples))?;
                }
                Write::Profile => {
                    m.select_profile(cfg.profile.active)?;
                    // The new profile may hold other buttons and acceleration: push the config's.
                    apply::apply(m, &cfg, cfg.polling.rate)?;
                }
                Write::Button(code, hs) => {
                    // A macro binding may shift the slot keys of other buttons: rewrite them all.
                    apply::apply_game_buttons(m, &cfg, None)?;
                    Self::write_hypershift_buttons(m, &cfg)?;
                    let _ = (code, hs);
                }
                Write::Macro(id) => {
                    if m.macro_support()? && !cfg.macro_needs_software(id) {
                        if let Some(mc) = cfg.macro_by_id(id) {
                            m.write_macro(id, &mc.name, &mc.bytes())?;
                        }
                    }
                    // Otherwise (no macro memory, or text / launch steps) the tray app plays it;
                    // it reloads the config after this commit.
                }
                Write::MacroDelete(id) => {
                    if m.macro_support()? && m.macro_ids()?.contains(&id) {
                        m.delete_macro(id)?;
                    }
                    // Buttons that pointed at it are back to their defaults in the config.
                    apply::apply_game_buttons(m, &cfg, None)?;
                    Self::write_hypershift_buttons(m, &cfg)?;
                }
                Write::Pair(act) => match act {
                    PairAct::Start => {
                        m.start_pairing(m.spec().product_id)?;
                    }
                    PairAct::Cancel => m.cancel_pairing(m.spec().product_id)?,
                    PairAct::Unpair => m.unpair(m.spec().product_id)?,
                    PairAct::Auto(on) => m.set_auto_pairing(on)?,
                },
            }
            Ok(())
        })();
        result
    }

    /// Back on the window's thread after a write: status line, pending state, the tray app.
    unsafe fn write_done(&mut self, write: Write, result: Result<(), HidError>, paired: Option<Option<Vec<PairedDevice>>>, auto: Option<bool>, storage: Option<MacroStorage>) {
        if let Write::Pair(act) = write {
            if result.is_ok() {
                self.pairing_since = if act == PairAct::Start { Some(Instant::now()) } else { None };
            }
        }
        if let Some(p) = paired {
            self.paired = p;
            self.auto_pair = auto;
        }
        if storage.is_some() {
            self.macro_storage = storage;
        }
        let applied = match result {
            Ok(()) => {
                self.connected = true;
                self.asleep = false;
                let msg = match write {
                    Write::Stages => "DPI saved to the mouse",
                    Write::Polling => "Polling rate saved to the mouse",
                    Write::Tracking => "Tracking settings saved to the mouse",
                    Write::Accel => "Dynamic Sensitivity saved to the mouse",
                    Write::Profile => "Onboard profile selected and the config written to it",
                    Write::Button(..) => "Button assignment saved to the mouse",
                    Write::Macro(_) => if self.macro_onboard == Some(true) { "Macro stored in the mouse" } else { "Macro saved; the tray app plays it when its button is pressed" },
                    Write::MacroDelete(_) => "Macro deleted",
                    Write::Pair(PairAct::Start) => "Dongle in pairing mode: switch the mouse off and on next to it",
                    Write::Pair(PairAct::Cancel) => "Pairing mode cancelled",
                    Write::Pair(PairAct::Unpair) => "Unpaired. Use \"Pair a device\" to pair the mouse again",
                    Write::Pair(PairAct::Auto(_)) => "Auto-pairing setting saved to the dongle",
                    _ => "Saved to the mouse",
                };
                self.set_status(msg, ACCENT);
                true
            }
            Err(HidError::Win32(..)) | Err(HidError::NotFound) => {
                self.connected = false;
                self.battery = None;
                self.pending = true;
                self.set_status("Saved. Mouse not connected: it will be applied when it is back", WARN);
                false
            }
            Err(HidError::Unsupported(e)) => {
                self.asleep = false;
                self.pending = false;
                self.set_status(&format!("Saved, but the mouse rejected it: {e}"), ERR);
                false
            }
            Err(e) if is_asleep_error(&e) => {
                self.asleep = true;
                self.pending = true;
                self.set_status("Saved. The mouse is asleep: move it and the change will be applied", WARN);
                false
            }
            Err(e) => {
                self.asleep = false;
                self.pending = false;
                self.set_status(&format!("Saved to config, but not applied to the mouse: {e}"), ERR);
                false
            }
        };
        if self.owner != 0 {
            PostMessageW(self.owner, WM_UI_CONFIG_CHANGED, applied as usize, 0);
        }
        self.repaint();
    }

    /// Write the Hypershift layer of the config (normal layer: `apply::apply_game_buttons`).
    fn write_hypershift_buttons(m: &Mouse, cfg: &Config) -> Result<(), HidError> {
        for (name, code) in BUTTONS {
            let key = format!("hypershift.{name}");
            let f = cfg.buttons.get(&key).and_then(|s| parse_button(s).ok().flatten()).unwrap_or(ButtonFunction::mouse_button(code));
            let f = apply::resolve(m, cfg, f)?;
            if m.button(m.profile(), code, true)? != f {
                m.set_button(m.profile(), code, true, f)?;
            }
        }
        Ok(())
    }

    /// Push the whole config once the mouse answers again (in the background).
    unsafe fn retry_pending(&mut self) {
        if self.retrying {
            return;
        }
        self.retrying = true;
        let cfg = self.cfg.clone();
        self.io(move |dev| {
            let result = dev.mouse().and_then(|m| apply::apply(m, &cfg, cfg.polling.rate));
            dev.check(&result);
            Box::new(move |ui: &mut Ui| {
                ui.retrying = false;
                match result {
                    Ok(changes) => {
                        ui.pending = false;
                        ui.asleep = false;
                        ui.connected = true;
                        ui.set_status(if changes.is_empty() { "The mouse is up to date" } else { "Pending changes applied to the mouse" }, ACCENT);
                        if ui.owner != 0 {
                            unsafe { PostMessageW(ui.owner, WM_UI_CONFIG_CHANGED, 1, 0) };
                        }
                        ui.read_live(false);
                    }
                    Err(e) if is_gone(&e) => ui.connected = false,
                    Err(e) if is_asleep_error(&e) => {}
                    Err(e) => {
                        ui.pending = false;
                        ui.asleep = false;
                        ui.set_status(&format!("Not applied to the mouse: {e}"), ERR);
                    }
                }
                ui.repaint();
            })
        });
    }

    unsafe fn reimport(&mut self) {
        self.set_status("Reading the mouse...", MUTED);
        self.io(|dev| {
            let result = dev.mouse().and_then(apply::import);
            dev.check(&result);
            Box::new(move |ui: &mut Ui| {
                match result {
                    Ok(mut cfg) => {
                        cfg.preserve_device_access(&config_path());
                        ui.cfg = cfg;
                        ui.last_good = ui.cfg.clone();
                        ui.cfg_error = None;
                        ui.pending = false;
                        if let Err(e) = Config::write_new(&config_path(), &ui.cfg) {
                            ui.set_status(&e, ERR);
                        } else {
                            ui.set_status("Config rebuilt from the mouse's onboard settings", ACCENT);
                        }
                        if ui.owner != 0 {
                            unsafe { PostMessageW(ui.owner, WM_UI_CONFIG_CHANGED, 1, 0) };
                        }
                    }
                    Err(HidError::NotFound) => ui.set_status("Mouse not connected", ERR),
                    Err(e) => ui.set_status(&format!("Could not read the mouse: {e}"), ERR),
                }
                ui.repaint();
            })
        });
    }

}

// Helpers used by the page builders.
impl Ui {
    pub(crate) fn config_error(&self) -> Option<&str> {
        self.cfg_error.as_deref()
    }

    pub(crate) fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    pub(crate) fn is_recording(&self) -> bool {
        self.recording.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_writes_are_not_sleep_failures() {
        assert!(!is_asleep_error(&HidError::Unverified("read-only".into())));
        assert!(!is_asleep_error(&HidError::Unsupported("not on this mouse")));
        assert!(!is_asleep_error(&HidError::Status(crate::protocol::Status::Failure)));
        assert!(is_asleep_error(&HidError::Status(crate::protocol::Status::Timeout)));
        assert!(is_asleep_error(&HidError::NoResponse));
    }

    #[test]
    fn dpi_input_respects_the_connected_mouses_limit() {
        let deathadder = DeviceSpec::lookup(0x00B6);
        let viper = DeviceSpec::lookup(0x00C0);
        assert_eq!(clamp_dpi(32000, &deathadder), 30000);
        assert_eq!(clamp_dpi(32000, &viper), 32000);
        let mut cfg = Config::default();
        cfg.dpi.stages = vec![32000];
        cfg.dpi.active = 1;
        assert!(cfg.validate_for(&deathadder).is_err());
        cfg.validate_for(&viper).unwrap();
    }
}
