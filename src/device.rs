//! High-level API for a Razer mouse (wired, or through its wireless dongle),
//! driven by the [`DeviceSpec`] table below. The Viper V3 Pro is the one
//! entry verified by the author; the others carry facts from OpenRazer's
//! device list and refuse writes until an owner confirms them.

use std::borrow::Cow;
use std::thread;
use std::time::Duration;

use crate::accel;
use crate::hid::{HidDevice, HidError, Result};
use crate::macros::{self, Playback};
use crate::protocol::{HID_BUF_LEN, Report, TID_DONGLE, TID_MOUSE, cmd};
use crate::sensor::{self, SensorPlan};

pub const VENDOR_RAZER: u16 = 0x1532;

pub const DPI_MIN: u16 = 100;
/// The rates the extended (HyperPolling) command can carry.
pub const RATES_HYPERPOLLING: [u16; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];
/// The rates of the classic polling command.
pub const RATES_CLASSIC: [u16; 3] = [125, 500, 1000];
/// What the HyperPolling command carries on a wired link (the Viper Mini SE, per OpenRazer).
const RATES_HYPERPOLLING_WIRED: [u16; 3] = [125, 500, 1000];
/// The Viper 8KHz: the extended command without 250 Hz (OpenRazer).
const RATES_VIPER_8K: [u16; 6] = [125, 500, 1000, 2000, 4000, 8000];

/// Which polling-rate command a mouse answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingStyle {
    /// `00/40` set (sent twice, with argument 0x00 then 0x01), `00/C0` get; codes 8000=0x01 .. 125=0x40.
    HyperPolling,
    /// `00/05` set, `00/85` get; codes 1000=0x01, 500=0x02, 250=0x04, 125=0x08.
    Classic,
}

/// What freenapse knows about one product id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSpec {
    pub product_id: u16,
    pub name: Cow<'static, str>,
    /// The mouse talks through a wireless dongle (dongle firmware, serial and LED commands apply).
    pub wireless: bool,
    /// Transaction id that addresses the mouse (0x1f on current firmware; 0x3f and 0xff on older lines).
    pub transaction_id: u8,
    pub polling: PollingStyle,
    pub polling_rates: &'static [u16],
    pub dpi_max: u16,
    /// The Focus Pro tracking page of `src/sensor.rs` (Smart Tracking / asymmetric cut-off) is known to apply.
    pub sensor_tracking: bool,
    /// Dynamic Sensitivity commands are available on this row.
    pub supports_acceleration: bool,
    /// Sensor angle commands are available on this row.
    pub supports_rotation: bool,
    /// Dongle pairing (and the dongle indicator LED) as Synapse offers them for the HyperPolling dongle.
    pub pairing: bool,
    /// An owner confirmed writes on this mouse. Until then every write is refused
    /// unless the config has `[device] allow_unverified = true`.
    pub verified: bool,
}

macro_rules! spec {
    ($pid:expr, $name:expr, wireless $w:expr, tid $tid:expr, $style:ident, $rates:expr, dpi $dpi:expr, sensor $s:expr, pairing $p:expr, verified $v:expr) => {
        spec!($pid, $name, wireless $w, tid $tid, $style, $rates, dpi $dpi, sensor $s, pairing $p, verified $v, acceleration true, rotation true)
    };
    ($pid:expr, $name:expr, wireless $w:expr, tid $tid:expr, $style:ident, $rates:expr, dpi $dpi:expr, sensor $s:expr, pairing $p:expr, verified $v:expr, acceleration $a:expr, rotation $r:expr) => {
        DeviceSpec {
            product_id: $pid,
            name: Cow::Borrowed($name),
            wireless: $w,
            transaction_id: $tid,
            polling: PollingStyle::$style,
            polling_rates: &$rates,
            dpi_max: $dpi,
            sensor_tracking: $s,
            supports_acceleration: $a,
            supports_rotation: $r,
            pairing: $p,
            verified: $v,
        }
    };
}

/// The device table. Row one is the mouse this project was written on; the
/// other rows take product id, transaction id, polling command and DPI maximum
/// from OpenRazer's device list (facts, no code) and stay unverified until an
/// owner's `freenapse probe` output confirms them (see CONTRIBUTING.md).
pub const DEVICES: &[DeviceSpec] = &[
    spec!(0x00C0, "Razer Viper V3 Pro (wired)", wireless false, tid 0x1f, Classic, RATES_CLASSIC, dpi 35000, sensor true, pairing false, verified true),
    spec!(0x00C1, "Razer Viper V3 Pro (wireless)", wireless true, tid 0x1f, HyperPolling, RATES_HYPERPOLLING, dpi 35000, sensor true, pairing true, verified true),
    spec!(0x00B3, "Razer HyperPolling wireless dongle", wireless true, tid 0x1f, HyperPolling, RATES_HYPERPOLLING, dpi 30000, sensor false, pairing true, verified false),
    spec!(0x00BE, "Razer DeathAdder V4 Pro (wired)", wireless false, tid 0x1f, HyperPolling, RATES_HYPERPOLLING, dpi 45000, sensor false, pairing false, verified false),
    spec!(0x00BF, "Razer DeathAdder V4 Pro (wireless)", wireless true, tid 0x1f, HyperPolling, RATES_HYPERPOLLING, dpi 45000, sensor false, pairing true, verified false),
    spec!(0x00B6, "Razer DeathAdder V3 Pro (wired)", wireless false, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false, acceleration false, rotation false),
    spec!(0x00B7, "Razer DeathAdder V3 Pro (wireless)", wireless true, tid 0x1f, Classic, RATES_CLASSIC, dpi 35000, sensor false, pairing false, verified false),
    spec!(0x00B2, "Razer DeathAdder V3", wireless false, tid 0x1f, HyperPolling, RATES_HYPERPOLLING, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x00A5, "Razer Viper V2 Pro (wired)", wireless false, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x00A6, "Razer Viper V2 Pro (wireless)", wireless true, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x00B8, "Razer Viper V3 HyperSpeed", wireless true, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x009E, "Razer Viper Mini SE (wired)", wireless false, tid 0x1f, HyperPolling, RATES_HYPERPOLLING_WIRED, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x009F, "Razer Viper Mini SE (wireless)", wireless true, tid 0x1f, HyperPolling, RATES_HYPERPOLLING, dpi 30000, sensor false, pairing true, verified false),
    spec!(0x0091, "Razer Viper 8KHz", wireless false, tid 0xff, HyperPolling, RATES_VIPER_8K, dpi 20000, sensor false, pairing false, verified false),
    spec!(0x00AF, "Razer Cobra Pro (wired)", wireless false, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x00B0, "Razer Cobra Pro (wireless)", wireless true, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x00AA, "Razer Basilisk V3 Pro (wired)", wireless false, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false),
    spec!(0x00AB, "Razer Basilisk V3 Pro (wireless)", wireless true, tid 0x1f, Classic, RATES_CLASSIC, dpi 30000, sensor false, pairing false, verified false),
];

impl DeviceSpec {
    /// The table row for a product id.
    pub fn find(product_id: u16) -> Option<&'static DeviceSpec> {
        DEVICES.iter().find(|d| d.product_id == product_id)
    }

    /// The table row, or the generic fallback for a Razer mouse that is not listed.
    pub fn lookup(product_id: u16) -> DeviceSpec {
        DeviceSpec::find(product_id).cloned().unwrap_or_else(|| DeviceSpec::unknown(product_id))
    }

    /// A Razer mouse that is not in the table: readable, writes refused, the
    /// most common transaction id and the classic polling command.
    pub fn unknown(product_id: u16) -> DeviceSpec {
        DeviceSpec {
            product_id,
            name: Cow::Owned(format!("Razer mouse 0x{product_id:04x}")),
            wireless: false,
            transaction_id: TID_MOUSE,
            polling: PollingStyle::Classic,
            polling_rates: &RATES_CLASSIC,
            dpi_max: 30000,
            sensor_tracking: false,
            supports_acceleration: true,
            supports_rotation: true,
            pairing: false,
            verified: false,
        }
    }

    /// The mouse this project was written on, through its dongle: what the
    /// config file is checked against when no mouse is open.
    pub fn viper_v3_pro() -> &'static DeviceSpec {
        &DEVICES[1]
    }

    /// True when the product id is in the table (verified or not).
    pub fn is_known(&self) -> bool {
        DeviceSpec::find(self.product_id).is_some()
    }

    pub fn dpi_ok(&self, dpi: u16) -> bool {
        (DPI_MIN..=self.dpi_max).contains(&dpi)
    }

    /// "125/250/500/1000/2000/4000/8000", for error messages.
    pub fn rates_text(&self) -> String {
        self.polling_rates.iter().map(u16::to_string).collect::<Vec<_>>().join("/")
    }

    /// Why a write to this mouse is refused, or None when it may go ahead.
    pub fn write_refusal(&self, allow_unverified: bool) -> Option<String> {
        if self.verified || allow_unverified {
            return None;
        }
        let why = if self.is_known() {
            format!("the {} is in freenapse's device table but nobody has confirmed writes on it yet", self.name)
        } else {
            format!("the {} (product id 0x{:04x}) is not in freenapse's device table", self.name, self.product_id)
        };
        Some(format!(
            "{why}, so it is read-only: run `freenapse probe` and open an issue at \
             https://github.com/MatthewSaintBull/freenapse/issues with its output. \
             To write to it anyway, at your own risk, set allow_unverified = true under [device] in the config file"
        ))
    }
}

impl Default for DeviceSpec {
    fn default() -> Self {
        DeviceSpec::viper_v3_pro().clone()
    }
}

/// Refuse a write (command id with bit 7 clear) to a mouse nobody has verified.
pub fn guard_write(spec: &DeviceSpec, allow_unverified: bool, req: &Report) -> Result<()> {
    if req.command_id & 0x80 != 0 {
        return Ok(());
    }
    match spec.write_refusal(allow_unverified) {
        Some(why) => Err(HidError::Unverified(why)),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpiStages {
    /// 1-based index of the active stage.
    pub active: u8,
    pub stages: Vec<(u16, u16)>,
}

/// What the sensor currently reports: calibration in use, lift mode and height code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorStatus {
    pub use_calibration: bool,
    pub mode: u8,
    pub height: u8,
}

impl SensorStatus {
    pub fn describe(&self) -> String {
        let m = match (self.mode, self.height) {
            (0, _) => "not set".to_string(),
            (1, h) => format!("smart tracking, {} mm", h + 1),
            (2, h) => format!("universal hysteresis mode, height {h} (not used by Synapse on this mouse)"),
            (3, _) => "preset lift cut-off mode".into(),
            (4, _) => "asymmetric cut-off (lift-off / landing levels are not readable)".into(),
            (5, _) => "manual calibration mode".into(),
            (6, _) => "manual calibration mode, asymmetric".into(),
            (m, h) => format!("mode {m} height {h}"),
        };
        format!("{m}{}", if self.use_calibration && self.mode != 4 { " (calibration data active)" } else { "" })
    }
}

/// Macro memory of the mouse, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroStorage {
    pub max_id: u16,
    pub total: u32,
    pub available: u32,
    pub recyclable: u32,
}

/// A device the dongle is paired with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairedDevice {
    pub product_id: u16,
    /// Link state byte as the dongle reports it (1 = connected on this mouse).
    pub status: u8,
}

/// One onboard button assignment (7 bytes: category, length, data[5]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonFunction(pub [u8; 7]);

impl ButtonFunction {
    pub const DISABLED: ButtonFunction = ButtonFunction([0x00, 0, 0, 0, 0, 0, 0]);
    pub fn mouse_button(code: u8) -> Self {
        ButtonFunction([0x01, 1, code, 0, 0, 0, 0])
    }
    pub fn key(modifiers: u8, hid_key: u8) -> Self {
        ButtonFunction([0x02, 2, modifiers, hid_key, 0, 0, 0])
    }
    pub fn consumer(usage: u16) -> Self {
        let b = usage.to_be_bytes();
        ButtonFunction([0x0a, 2, b[0], b[1], 0, 0, 0])
    }
    /// 0x01 = next stage, 0x02 = previous (both wrap), 0x03 + level = jump to a stage.
    pub fn dpi_cycle_up() -> Self {
        ButtonFunction([0x06, 1, 0x01, 0, 0, 0, 0])
    }
    pub fn dpi_cycle_down() -> Self {
        ButtonFunction([0x06, 1, 0x02, 0, 0, 0, 0])
    }
    pub fn dpi_stage(level: u8) -> Self {
        ButtonFunction([0x06, 2, 0x03, level, 0, 0, 0])
    }
    /// 0x01 next, 0x02 previous, 0x03 + id, 0x04 cycle up, 0x05 cycle down.
    pub fn profile_next() -> Self {
        ButtonFunction([0x07, 1, 0x01, 0, 0, 0, 0])
    }
    pub fn hypershift() -> Self {
        ButtonFunction([0x0c, 1, 0x01, 0, 0, 0, 0])
    }
    /// Synapse's "Double Click": two left clicks per press.
    pub fn double_click() -> Self {
        ButtonFunction([0x0b, 1, 0x01, 0, 0, 0, 0])
    }
    /// Repeat a mouse button while held, `per_second` times a second.
    pub fn turbo_button(code: u8, per_second: u8) -> Self {
        let d = turbo_delay(per_second).to_be_bytes();
        ButtonFunction([0x0e, 3, code, d[0], d[1], 0, 0])
    }
    /// Repeat a key while held.
    pub fn turbo_key(modifiers: u8, hid_key: u8, per_second: u8) -> Self {
        let d = turbo_delay(per_second).to_be_bytes();
        ButtonFunction([0x0d, 4, modifiers, hid_key, d[0], d[1], 0])
    }
    /// System keys: [`POWER_SLEEP`], [`POWER_OFF`], [`POWER_WAKE`].
    pub fn power_key(code: u8) -> Self {
        ButtonFunction([0x09, 1, code, 0, 0, 0, 0])
    }
    /// Onboard macro with one of Synapse's playback options.
    pub fn macro_play(id: u16, play: Playback) -> Self {
        let b = id.to_be_bytes();
        match play {
            Playback::Times(n) => ButtonFunction([0x03, 3, b[0], b[1], n.max(1), 0, 0]),
            Playback::Held => ButtonFunction([0x04, 2, b[0], b[1], 0, 0, 0]),
            Playback::Toggle => ButtonFunction([0x05, 2, b[0], b[1], 0, 0, 0]),
            Playback::Sequence => ButtonFunction([0x0f, 2, b[0], b[1], 0, 0, 0]),
        }
    }

    /// (macro id, playback) when this is a macro function.
    pub fn macro_binding(&self) -> Option<(u16, Playback)> {
        let d = &self.0;
        let id = u16::from_be_bytes([d[2], d[3]]);
        Some(match d[0] {
            0x03 => (id, Playback::Times(d[4].max(1))),
            0x04 => (id, Playback::Held),
            0x05 => (id, Playback::Toggle),
            0x0f => (id, Playback::Sequence),
            _ => return None,
        })
    }

    /// Repeat rate of a turbo function, in presses per second.
    pub fn turbo_rate(&self) -> Option<u8> {
        let d = &self.0;
        let delay = match d[0] {
            0x0e => u16::from_be_bytes([d[3], d[4]]),
            0x0d => u16::from_be_bytes([d[4], d[5]]),
            _ => return None,
        };
        Some(((1000.0 / delay.max(1) as f32).round() as u8).clamp(TURBO_MIN, TURBO_MAX))
    }

    pub fn describe(&self) -> String {
        let d = &self.0;
        match d[0] {
            0x00 => "disabled".into(),
            0x01 => format!("mouse button {}", mouse_button_name(d[2])),
            0x0b => "double-click".into(),
            0x0e => format!("turbo {} every {} ms", mouse_button_name(d[2]), u16::from_be_bytes([d[3], d[4]])),
            0x02 => format!("key 0x{:02x} (mods 0x{:02x})", d[3], d[2]),
            0x0d => format!("turbo key 0x{:02x} (mods 0x{:02x}) every {} ms", d[3], d[2], u16::from_be_bytes([d[4], d[5]])),
            0x09 => match d[2] {
                POWER_OFF => "system power".into(),
                POWER_SLEEP => "system sleep".into(),
                POWER_WAKE => "system wake".into(),
                x => format!("system key 0x{x:02x}"),
            },
            0x0a => format!("media key 0x{:04x}", u16::from_be_bytes([d[2], d[3]])),
            0x03..=0x05 | 0x0f => {
                let (id, play) = self.macro_binding().unwrap_or((0, Playback::Times(1)));
                format!("macro {id} ({})", play.label().to_lowercase())
            }
            0x06 => match d[2] {
                0x01 => "DPI cycle up".into(),
                0x02 => "DPI cycle down".into(),
                0x03 => format!("DPI stage {}", d[3]),
                x => format!("DPI op 0x{x:02x}"),
            },
            0x07 => match d[2] {
                0x01 => "profile next".into(),
                0x02 => "profile previous".into(),
                0x03 => format!("profile {}", d[3]),
                0x04 => "profile cycle up".into(),
                0x05 => "profile cycle down".into(),
                x => format!("profile op 0x{x:02x}"),
            },
            0x0c => "hypershift".into(),
            0x12 => "scroll mode".into(),
            x => format!("unknown 0x{x:02x} {:02x?}", &d[1..]),
        }
    }
}

/// System key codes of the power-key function (Synapse: SystemPower, SystemSleep, SystemWake).
pub const POWER_OFF: u8 = 0x81;
pub const POWER_SLEEP: u8 = 0x82;
pub const POWER_WAKE: u8 = 0x83;

/// Turbo repeat rate range, presses per second.
pub const TURBO_MIN: u8 = 1;
pub const TURBO_MAX: u8 = 50;

/// Interval in ms between repeats, as Synapse computes it (`1000 / keysPerSecond`).
pub fn turbo_delay(per_second: u8) -> u16 {
    1000 / per_second.clamp(TURBO_MIN, TURBO_MAX) as u16
}

pub fn mouse_button_name(code: u8) -> &'static str {
    match code {
        0x01 => "left",
        0x02 => "right",
        0x03 => "middle",
        0x04 => "back",
        0x05 => "forward",
        0x09 => "scroll up",
        0x0a => "scroll down",
        _ => "?",
    }
}

/// Physical buttons of the Viper V3 Pro as addressed by the button-mapping command.
pub const BUTTONS: [(&str, u8); 7] =
    [("left", 0x01), ("right", 0x02), ("middle", 0x03), ("back", 0x04), ("forward", 0x05), ("scroll-up", 0x09), ("scroll-down", 0x0a)];

pub fn button_code(name: &str) -> Option<u8> {
    BUTTONS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

pub struct Mouse {
    hid: HidDevice,
    spec: DeviceSpec,
    /// `[device] allow_unverified` of the config file: writes to an unverified mouse go ahead.
    allow_unverified: bool,
    /// Onboard profile that button and acceleration commands address.
    profile: std::cell::Cell<u8>,
    /// Whether the mouse has macro memory (None until asked).
    macro_support: std::cell::Cell<Option<bool>>,
}

impl Mouse {
    /// Open the first Razer mouse that is in the device table, or failing that
    /// any Razer device with a 90-byte control report as [`DeviceSpec::unknown`].
    /// Whether writes to an unverified mouse are allowed comes from the config file.
    pub fn open() -> Result<Mouse> {
        Mouse::open_with(crate::config::Config::allow_unverified_at(&crate::config_path()))
    }

    pub fn open_with(allow_unverified: bool) -> Result<Mouse> {
        let known: Vec<u16> = DEVICES.iter().map(|d| d.product_id).collect();
        let hid = match HidDevice::open_control(VENDOR_RAZER, &known) {
            Ok(h) => h,
            Err(HidError::NotFound) => HidDevice::open_control_any(VENDOR_RAZER, |info| {
                // Not a keyboard's control collection (usage page 1, usage 6).
                info.feature_report_len as usize >= HID_BUF_LEN && !(info.usage_page == 1 && info.usage == 6)
            })?,
            Err(e) => return Err(e),
        };
        let spec = DeviceSpec::lookup(hid.info.product_id);
        Ok(Mouse { hid, spec, allow_unverified, profile: std::cell::Cell::new(1), macro_support: std::cell::Cell::new(None) })
    }

    /// What the table says about this mouse.
    pub fn spec(&self) -> &DeviceSpec {
        &self.spec
    }

    /// False when every write would be refused (unverified mouse, flag not set).
    pub fn writes_allowed(&self) -> bool {
        self.spec.write_refusal(self.allow_unverified).is_none()
    }

    pub fn product_id(&self) -> u16 {
        self.hid.info.product_id
    }

    pub fn vendor_id(&self) -> u16 {
        self.hid.info.vendor_id
    }

    /// True when the mouse stores macros itself. The Viper V3 Pro answers
    /// "not supported" to every macro command, so its macros are played by the
    /// tray app instead. Errors other than NotSupported (asleep) are propagated.
    pub fn macro_support(&self) -> Result<bool> {
        if let Some(v) = self.macro_support.get() {
            return Ok(v);
        }
        let v = match self.raw(&macros::get_storage_info()) {
            Ok(_) => true,
            Err(HidError::Status(crate::protocol::Status::NotSupported)) => false,
            Err(e) => return Err(e),
        };
        self.macro_support.set(Some(v));
        Ok(v)
    }

    /// The onboard profile this handle writes to and reads from (1 until [`Mouse::select_profile`]).
    pub fn profile(&self) -> u8 {
        self.profile.get()
    }

    pub fn is_wireless(&self) -> bool {
        self.spec.wireless
    }

    pub fn path(&self) -> &str {
        &self.hid.info.path
    }

    pub fn set_wait_ms(&mut self, ms: u64) {
        self.hid.wait = Duration::from_millis(ms);
    }

    /// Send a command. A request addressed to [`TID_MOUSE`] is re-addressed to
    /// this mouse's transaction id (a no-op on the Viper V3 Pro); writes to an
    /// unverified mouse are refused, see [`DeviceSpec::write_refusal`].
    pub fn raw(&self, req: &Report) -> Result<Report> {
        if req.transaction_id == TID_MOUSE && self.spec.transaction_id != TID_MOUSE {
            let mut r = req.clone();
            r.transaction_id = self.spec.transaction_id;
            return self.raw_as_is(&r);
        }
        self.raw_as_is(req)
    }

    /// Send a command with the transaction id it carries, no re-addressing.
    pub fn raw_as_is(&self, req: &Report) -> Result<Report> {
        guard_write(&self.spec, self.allow_unverified, req)?;
        self.hid.transact(req)
    }

    fn need_dongle(&self) -> Result<()> {
        if self.is_wireless() { Ok(()) } else { Err(HidError::Unsupported("only available through the wireless dongle")) }
    }

    fn need_pairing(&self) -> Result<()> {
        self.need_dongle()?;
        if self.spec.pairing { Ok(()) } else { Err(HidError::Unsupported("dongle pairing is not known to apply to this mouse")) }
    }

    // --- identity ---

    pub fn firmware(&self) -> Result<(u8, u8)> {
        let r = self.raw(&cmd::firmware_version(TID_MOUSE))?;
        Ok((r.arguments[0], r.arguments[1]))
    }

    pub fn serial(&self) -> Result<String> {
        let r = self.raw(&cmd::serial(TID_MOUSE))?;
        Ok(ascii(&r.arguments[..22]))
    }

    pub fn dongle_firmware(&self) -> Result<(u8, u8)> {
        self.need_dongle()?;
        let r = self.raw(&cmd::firmware_version(TID_DONGLE))?;
        Ok((r.arguments[0], r.arguments[1]))
    }

    pub fn dongle_serial(&self) -> Result<String> {
        self.need_dongle()?;
        let r = self.raw(&cmd::serial(TID_DONGLE))?;
        Ok(ascii(&r.arguments[..22]))
    }

    pub fn device_mode(&self) -> Result<u8> {
        Ok(self.raw(&cmd::get_device_mode())?.arguments[0])
    }

    pub fn set_device_mode(&self, mode: u8) -> Result<()> {
        self.raw(&cmd::set_device_mode(mode)).map(drop)
    }

    // --- power ---

    pub fn battery(&self) -> Result<Battery> {
        let level = self.raw(&cmd::get_battery_level())?;
        let charging = self.raw(&cmd::get_charging_status())?;
        Ok(Battery { percent: cmd::byte_to_percent(level.arguments[1]), charging: charging.arguments[1] != 0 })
    }

    /// Seconds of inactivity before the mouse sleeps (60..=900).
    pub fn idle_time(&self) -> Result<u16> {
        let r = self.raw(&cmd::get_idle_time())?;
        Ok(u16::from_be_bytes([r.arguments[0], r.arguments[1]]))
    }

    pub fn set_idle_time(&self, seconds: u16) -> Result<()> {
        self.raw(&cmd::set_idle_time(seconds)).map(drop)
    }

    /// Low battery warning threshold in percent.
    pub fn low_battery_threshold(&self) -> Result<u8> {
        Ok(cmd::byte_to_percent(self.raw(&cmd::get_low_battery_threshold())?.arguments[0]))
    }

    pub fn set_low_battery_threshold(&self, percent: u8) -> Result<()> {
        self.raw(&cmd::set_low_battery_threshold(percent)).map(drop)
    }

    // --- performance ---

    pub fn dpi(&self) -> Result<(u16, u16)> {
        let a = self.raw(&cmd::get_dpi_xy())?.arguments;
        Ok((u16::from_be_bytes([a[1], a[2]]), u16::from_be_bytes([a[3], a[4]])))
    }

    pub fn set_dpi(&self, x: u16, y: u16) -> Result<()> {
        let max = self.spec.dpi_max;
        self.raw(&cmd::set_dpi_xy(x.clamp(DPI_MIN, max), y.clamp(DPI_MIN, max))).map(drop)
    }

    pub fn dpi_stages(&self) -> Result<DpiStages> {
        let r = self.raw(&cmd::get_dpi_stages())?;
        let a = &r.arguments;
        let active = a[1];
        let count = (a[2] as usize).min(5);
        let mut stages = Vec::with_capacity(count);
        for i in 0..count {
            let o = 3 + i * 7;
            stages.push((u16::from_be_bytes([a[o + 1], a[o + 2]]), u16::from_be_bytes([a[o + 3], a[o + 4]])));
        }
        Ok(DpiStages { active, stages })
    }

    pub fn set_dpi_stages(&self, active: u8, stages: &[(u16, u16)]) -> Result<()> {
        let max = self.spec.dpi_max;
        let stages: Vec<(u16, u16)> = stages.iter().take(5).map(|(x, y)| (x.clamp(&DPI_MIN, &max).to_owned(), y.clamp(&DPI_MIN, &max).to_owned())).collect();
        let active = active.clamp(1, stages.len().max(1) as u8);
        self.raw(&cmd::set_dpi_stages(active, &stages))?;
        // Keep the live DPI in sync with the selected stage, like Synapse does.
        if let Some((x, y)) = stages.get(active as usize - 1) {
            self.set_dpi(*x, *y)?;
        }
        Ok(())
    }

    pub fn polling_rate(&self) -> Result<u16> {
        match self.spec.polling {
            PollingStyle::HyperPolling => {
                let a = self.raw(&cmd::get_polling_rate())?.arguments;
                cmd::polling_from_code(a[1]).or_else(|| cmd::polling_from_code(a[0])).ok_or(HidError::NoResponse)
            }
            PollingStyle::Classic => {
                let a = self.raw(&cmd::get_polling_rate_classic())?.arguments;
                Ok(cmd::polling_from_code_classic(a[0]).unwrap_or(0))
            }
        }
    }

    pub fn set_polling_rate(&self, hz: u16) -> Result<()> {
        if !self.spec.polling_rates.contains(&hz) {
            return Err(HidError::Invalid(format!("polling rate must be one of {} on the {}", self.spec.rates_text(), self.spec.name)));
        }
        match self.spec.polling {
            PollingStyle::HyperPolling => {
                self.raw(&cmd::set_polling_rate(hz, 0x00))?;
                self.raw(&cmd::set_polling_rate(hz, 0x01))?;
                Ok(())
            }
            PollingStyle::Classic => {
                let code = cmd::polling_code_classic(hz).ok_or(HidError::Unsupported("the classic polling command has no code for that rate"))?;
                self.raw(&cmd::set_polling_rate_classic(code)).map(drop)
            }
        }
    }

    // --- dongle ---

    pub fn dongle_led_mode(&self) -> Result<u8> {
        self.need_dongle()?;
        Ok(self.raw(&cmd::get_dongle_led_mode())?.arguments[0])
    }

    pub fn set_dongle_led_mode(&self, mode: u8) -> Result<()> {
        self.need_dongle()?;
        self.raw(&cmd::set_dongle_led_mode(mode)).map(drop)
    }

    // --- sensor ---

    pub fn sensor(&self) -> Result<SensorStatus> {
        let lift = self.raw(&sensor::get_lift())?.arguments;
        let state = self.raw(&sensor::get_state())?.arguments;
        Ok(SensorStatus { use_calibration: state[2] != 0, mode: lift[2], height: lift[3] })
    }

    /// Write a tracking plan in the order Synapse's engine for this mouse does.
    pub fn apply_tracking(&self, plan: &SensorPlan) -> Result<()> {
        if !self.spec.sensor_tracking {
            return Err(HidError::Unsupported("the sensor tracking page is not known to apply to this mouse"));
        }
        // Firmware 1.11 answers "Failure" to a lift-setting write that repeats the
        // mode it already has, although it applies it, so the status of that one
        // write is not trusted: the result is read back at the end instead.
        let tolerant = |r: &crate::protocol::Report| match self.raw(r) {
            Err(crate::hid::HidError::Status(crate::protocol::Status::Failure)) => Ok(()),
            other => other.map(drop),
        };
        if plan.state {
            // Asymmetric cut-off: state on, lift mode 4, then the two level bytes.
            self.raw(&sensor::set_state(true))?;
            tolerant(&sensor::set_lift(plan.lift.0, plan.lift.1))?;
            if let Some(cfg) = &plan.config {
                self.raw(&sensor::set_config(cfg))?;
            }
        } else {
            // Smart Tracking: state off first; with the state on the firmware
            // drops a mode-1 write to height 0 instead of applying it.
            self.raw(&sensor::set_state(false))?;
            tolerant(&sensor::set_lift(plan.lift.0, plan.lift.1))?;
        }
        let s = self.sensor()?;
        if s.mode != plan.lift.0 || s.height != plan.lift.1 || s.use_calibration != plan.state {
            return Err(crate::hid::HidError::Status(crate::protocol::Status::Failure));
        }
        Ok(())
    }

    /// Sensor rotation: (enabled, degrees).
    pub fn rotation(&self) -> Result<(bool, i8)> {
        if !self.spec.supports_rotation {
            return Err(HidError::Unsupported("sensor rotation is not supported on this mouse"));
        }
        let a = self.raw(&sensor::get_angle())?.arguments;
        Ok((a[1] != 0, a[2] as i8))
    }

    pub fn set_rotation(&self, degrees: i8) -> Result<()> {
        if !self.spec.supports_rotation {
            return Err(HidError::Unsupported("sensor rotation is not supported on this mouse"));
        }
        self.raw(&sensor::set_angle(degrees != 0, degrees)).map(drop)
    }

    /// Raw reads of every sensor register we know about, for diagnostics.
    pub fn sensor_dump(&self) -> Vec<(&'static str, Result<Vec<u8>>)> {
        let regs: [(&str, Report); 4] = [
            ("state   0b/83", sensor::get_state()),
            ("lift    0b/8b", sensor::get_lift()),
            ("config  0b/85", sensor::get_config()),
            ("angle   0b/94", sensor::get_angle()),
        ];
        regs.into_iter()
            .map(|(name, req)| {
                let n = req.data_size as usize;
                (name, self.raw(&req).map(|r| r.arguments[..n].to_vec()))
            })
            .collect()
    }

    // --- dynamic sensitivity (onboard acceleration) ---

    /// (enabled, mode) stored in the mouse.
    pub fn acceleration(&self) -> Result<(bool, accel::Mode)> {
        if !self.spec.supports_acceleration {
            return Err(HidError::Unsupported("Dynamic Sensitivity is not supported on this mouse"));
        }
        let p = self.profile();
        let st = self.raw(&accel::get_state(p))?.arguments;
        let md = self.raw(&accel::get_mode(p))?.arguments;
        Ok((st[1] != 0, accel::Mode::from_byte(md[1]).unwrap_or(accel::Mode::Classic)))
    }

    /// Write the acceleration settings in Synapse's order: state, then (when
    /// on) the mode, then the custom curve in 32 packets of 8 samples.
    pub fn set_acceleration(&self, on: bool, mode: accel::Mode, curve: Option<&[f32; accel::POINTS]>) -> Result<()> {
        if !self.spec.supports_acceleration {
            return Err(HidError::Unsupported("Dynamic Sensitivity is not supported on this mouse"));
        }
        let p = self.profile();
        self.raw(&accel::set_state(p, on))?;
        if !on {
            return Ok(());
        }
        self.raw(&accel::set_mode(p, mode))?;
        if mode == accel::Mode::Custom {
            if let Some(c) = curve {
                for (i, chunk) in c.chunks(accel::CHUNK).enumerate() {
                    self.raw(&accel::set_curve_chunk(p, mode, (i * accel::CHUNK) as u8, chunk))?;
                    thread::sleep(Duration::from_millis(15));
                }
            }
        }
        Ok(())
    }

    /// Read `count` samples of the stored custom curve starting at `offset`.
    pub fn acceleration_curve_chunk(&self, offset: u8, count: u8) -> Result<Vec<f32>> {
        if !self.spec.supports_acceleration {
            return Err(HidError::Unsupported("Dynamic Sensitivity is not supported on this mouse"));
        }
        let a = self.raw(&accel::get_curve_chunk(self.profile(), accel::Mode::Custom, offset, count))?.arguments;
        let n = (a[4] as usize).min(accel::CHUNK);
        Ok((0..n).map(|i| f32::from_le_bytes([a[5 + i * 4], a[6 + i * 4], a[7 + i * 4], a[8 + i * 4]])).collect())
    }

    /// The whole stored custom curve (32 reads).
    pub fn acceleration_curve(&self) -> Result<[f32; accel::POINTS]> {
        let mut out = [0f32; accel::POINTS];
        for i in (0..accel::POINTS).step_by(accel::CHUNK) {
            let v = self.acceleration_curve_chunk(i as u8, accel::CHUNK as u8)?;
            out[i..i + v.len()].copy_from_slice(&v);
        }
        Ok(out)
    }

    // --- onboard profiles ---

    /// (existing profiles, maximum the mouse supports).
    pub fn profiles(&self) -> Result<(u8, u8)> {
        let count = self.raw(&cmd::get_profile_count())?.arguments[0];
        let max = self.raw(&cmd::get_max_profiles())?.arguments[0];
        Ok((count, max))
    }

    /// Ids of the profiles stored in the mouse.
    pub fn profile_list(&self) -> Result<Vec<u8>> {
        let a = self.raw(&cmd::get_profile_list())?.arguments;
        let n = (a[0] as usize).min(5);
        Ok(a[1..1 + n].to_vec())
    }

    /// The profile the mouse itself reports as active (not answered by the Viper V3 Pro).
    pub fn active_profile(&self) -> Result<u8> {
        Ok(self.raw(&cmd::get_active_profile())?.arguments[0])
    }

    /// Make `id` the profile this handle addresses and the mouse's active one:
    /// creates the slot when it is missing and the mouse has room, and skips
    /// the activation when the mouse holds that single profile already.
    pub fn select_profile(&self, id: u8) -> Result<()> {
        if !(1..=5).contains(&id) {
            return Err(HidError::Unsupported("onboard profile id must be 1..5"));
        }
        let (_, max) = self.profiles()?;
        let list = self.profile_list()?;
        if !list.contains(&id) {
            if id > max {
                return Err(HidError::Unsupported("the mouse does not have that many onboard profiles"));
            }
            self.raw(&cmd::create_profile(id))?;
        }
        if !(list.len() == 1 && list[0] == id) {
            self.raw(&cmd::set_active_profile(id))?;
        }
        self.profile.set(id);
        Ok(())
    }

    // --- buttons ---

    pub fn button(&self, profile: u8, button: u8, hypershift: bool) -> Result<ButtonFunction> {
        let a = self.raw(&cmd::get_button_function(profile, button, hypershift))?.arguments;
        let mut f = [0u8; 7];
        f.copy_from_slice(&a[3..10]);
        Ok(ButtonFunction(f))
    }

    pub fn set_button(&self, profile: u8, button: u8, hypershift: bool, f: ButtonFunction) -> Result<()> {
        self.raw(&cmd::set_button_function(profile, button, hypershift, &f.0)).map(drop)
    }

    // --- onboard macros ---

    pub fn macro_storage(&self) -> Result<MacroStorage> {
        let a = self.raw(&macros::get_storage_info())?.arguments;
        let dw = |o: usize| u32::from_be_bytes([a[o], a[o + 1], a[o + 2], a[o + 3]]);
        Ok(MacroStorage { max_id: u16::from_be_bytes([a[0], a[1]]), total: dw(2), available: dw(6), recyclable: dw(10) })
    }

    /// Ids of the macros stored in the mouse.
    pub fn macro_ids(&self) -> Result<Vec<u16>> {
        let a = self.raw(&macros::get_id_list())?.arguments;
        let n = (u16::from_be_bytes([a[2], a[3]]) as usize).min(38);
        Ok((0..n).map(|i| u16::from_be_bytes([a[4 + 2 * i], a[5 + 2 * i]])).collect())
    }

    pub fn macro_data_size(&self, id: u16) -> Result<u32> {
        let a = self.raw(&macros::get_data_size(id))?.arguments;
        Ok(u32::from_be_bytes([a[2], a[3], a[4], a[5]]))
    }

    /// The stored event bytes of a macro.
    pub fn macro_data(&self, id: u16) -> Result<Vec<u8>> {
        let size = self.macro_data_size(id)? as usize;
        let mut out = Vec::with_capacity(size);
        while out.len() < size {
            let want = (size - out.len()).min(macros::DATA_CHUNK) as u8;
            let a = self.raw(&macros::get_data_chunk(id, out.len() as u32, want))?.arguments;
            let got = (a[6] as usize).min(macros::DATA_CHUNK);
            if got == 0 {
                break;
            }
            out.extend_from_slice(&a[7..7 + got]);
            thread::sleep(Duration::from_millis(10));
        }
        Ok(out)
    }

    /// The name stored in the macro's name structure.
    pub fn macro_name(&self, id: u16) -> Result<String> {
        let a = self.raw(&macros::get_name_chunk(id, 0))?.arguments;
        Ok(macros::name_from_structure(&a[6..]))
    }

    /// Store a macro, replacing any with the same id, in Synapse's order:
    /// allocate, name structure in 64-byte chunks, event data in 72-byte chunks.
    pub fn write_macro(&self, id: u16, name: &str, data: &[u8]) -> Result<()> {
        if self.macro_ids()?.contains(&id) {
            self.raw(&macros::delete(id))?;
            thread::sleep(Duration::from_millis(20));
        }
        let st = self.macro_storage()?;
        if macros::required_memory(data.len()) > st.available {
            return Err(HidError::Unsupported("not enough macro memory in the mouse: delete a macro first"));
        }
        self.raw(&macros::allocate(id, data.len() as u32))?;
        thread::sleep(Duration::from_millis(10));
        let s = macros::name_structure(id, name, data.len() as u32);
        for off in (0..macros::NAME_STRUCT_LEN).step_by(macros::NAME_CHUNK) {
            let end = (off + macros::NAME_CHUNK).min(macros::NAME_STRUCT_LEN);
            self.raw(&macros::set_name_chunk(id, off as u16, macros::NAME_STRUCT_LEN as u16, &s[off..end]))?;
            thread::sleep(Duration::from_millis(10));
        }
        for off in (0..data.len()).step_by(macros::DATA_CHUNK) {
            let end = (off + macros::DATA_CHUNK).min(data.len());
            self.raw(&macros::set_data_chunk(id, off as u32, &data[off..end]))?;
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    pub fn delete_macro(&self, id: u16) -> Result<()> {
        self.raw(&macros::delete(id)).map(drop)
    }

    // --- dongle pairing ---

    /// Devices the dongle is paired with.
    pub fn paired_devices(&self) -> Result<Vec<PairedDevice>> {
        self.need_pairing()?;
        let a = self.raw(&cmd::get_paired_devices(TID_DONGLE))?.arguments;
        let n = (a[0] as usize).min(8);
        // Empty slots are reported as product id 0xffff.
        Ok((0..n)
            .map(|i| PairedDevice { status: a[1 + 3 * i], product_id: u16::from_be_bytes([a[2 + 3 * i], a[3 + 3 * i]]) })
            .filter(|d| d.product_id != 0xffff && d.product_id != 0)
            .collect())
    }

    /// Put the dongle in pairing mode for a device with this product id. Returns the mode byte it answers.
    pub fn start_pairing(&self, product_id: u16) -> Result<u8> {
        self.need_pairing()?;
        Ok(self.raw(&cmd::set_pairing_mode(TID_DONGLE, cmd::PAIRING_START, product_id))?.arguments[0])
    }

    pub fn cancel_pairing(&self, product_id: u16) -> Result<()> {
        self.need_pairing()?;
        self.raw(&cmd::set_pairing_mode(TID_DONGLE, cmd::PAIRING_CANCEL, product_id)).map(drop)
    }

    /// Forget the pairing with the device that has this product id.
    pub fn unpair(&self, product_id: u16) -> Result<()> {
        self.need_pairing()?;
        self.raw(&cmd::unpair(TID_DONGLE, product_id)).map(drop)
    }

    pub fn auto_pairing(&self) -> Result<bool> {
        self.need_pairing()?;
        Ok(self.raw(&cmd::get_auto_pairing(TID_DONGLE))?.arguments[0] != 0)
    }

    pub fn set_auto_pairing(&self, on: bool) -> Result<()> {
        self.need_pairing()?;
        self.raw(&cmd::set_auto_pairing(TID_DONGLE, on)).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_lookup() {
        let wired = DeviceSpec::lookup(0x00C0);
        assert_eq!(wired.name, "Razer Viper V3 Pro (wired)");
        assert!(!wired.wireless && !wired.pairing && wired.verified);
        assert_eq!((wired.polling, wired.polling_rates), (PollingStyle::Classic, &RATES_CLASSIC[..]));
        let wireless = DeviceSpec::lookup(0x00C1);
        assert_eq!(wireless, *DeviceSpec::viper_v3_pro());
        assert!(wireless.wireless && wireless.pairing && wireless.sensor_tracking && wireless.verified);
        assert_eq!(
            (wireless.polling, wireless.polling_rates, wireless.dpi_max, wireless.transaction_id),
            (PollingStyle::HyperPolling, &RATES_HYPERPOLLING[..], 35000, TID_MOUSE)
        );
        assert_eq!(DeviceSpec::default(), wireless);
        assert_eq!(wireless.rates_text(), "125/250/500/1000/2000/4000/8000");
        assert!(wireless.dpi_ok(35000) && !wireless.dpi_ok(35001) && !wireless.dpi_ok(99));
        // Every row has a distinct product id and a non-empty rate list.
        for (i, d) in DEVICES.iter().enumerate() {
            assert!(!d.polling_rates.is_empty(), "{}", d.name);
            assert!(DEVICES[..i].iter().all(|o| o.product_id != d.product_id), "duplicate 0x{:04x}", d.product_id);
        }
        // Only the author's mouse is verified so far.
        assert_eq!(DEVICES.iter().filter(|d| d.verified).count(), 2);
    }

    #[test]
    fn spec_unknown() {
        let u = DeviceSpec::lookup(0x0123);
        assert_eq!(u, DeviceSpec::unknown(0x0123));
        assert_eq!(u.name, "Razer mouse 0x0123");
        assert!(!u.is_known() && !u.verified && !u.wireless && !u.pairing && !u.sensor_tracking);
        assert_eq!((u.polling, u.dpi_max, u.transaction_id), (PollingStyle::Classic, 30000, TID_MOUSE));
        assert!(DeviceSpec::lookup(0x00B7).is_known());
    }

    #[test]
    fn deathadder_wired_capabilities_are_conservative() {
        let d = DeviceSpec::lookup(0x00B6);
        assert!(!d.verified && !d.wireless);
        assert!(!d.sensor_tracking, "successful sensor reads do not establish safe writes");
        assert!(!d.supports_acceleration && !d.supports_rotation);
        assert_eq!((d.transaction_id, d.polling, d.polling_rates, d.dpi_max), (0x1f, PollingStyle::Classic, &RATES_CLASSIC[..], 30000));
        assert!(matches!(guard_write(&d, false, &cmd::set_polling_rate_classic(1)), Err(HidError::Unverified(_))));
        assert!(guard_write(&d, true, &cmd::set_polling_rate_classic(1)).is_ok());
        let wireless = DeviceSpec::lookup(0x00B7);
        assert!(!wireless.verified && wireless.supports_acceleration && wireless.supports_rotation);
        assert_eq!(wireless.dpi_max, 35000);
        assert!(!wireless.sensor_tracking);
        assert!(DeviceSpec::viper_v3_pro().supports_acceleration);
        assert!(DeviceSpec::viper_v3_pro().supports_rotation);
    }

    #[test]
    fn unknown_device_refuses_writes() {
        let unknown = DeviceSpec::unknown(0x0123);
        let read = cmd::get_dpi_xy();
        let write = cmd::set_dpi_xy(800, 800);
        assert!(guard_write(&unknown, false, &read).is_ok());
        match guard_write(&unknown, false, &write) {
            Err(HidError::Unverified(why)) => {
                assert!(why.contains("freenapse probe") && why.contains("0x0123") && why.contains("allow_unverified"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        assert!(guard_write(&unknown, true, &write).is_ok());
        // A table row that nobody verified is refused too, with its name.
        let listed = DeviceSpec::lookup(0x00B7);
        let why = listed.write_refusal(false).unwrap();
        assert!(why.contains("DeathAdder V3 Pro") && why.contains("not confirmed") || why.contains("nobody has confirmed"), "{why}");
        assert!(guard_write(&listed, false, &write).is_err());
        // The verified mouse writes freely.
        assert!(guard_write(DeviceSpec::viper_v3_pro(), false, &write).is_ok());
        assert!(guard_write(&DeviceSpec::lookup(0x00C0), false, &write).is_ok());
    }
}

fn ascii(bytes: &[u8]) -> String {
    bytes.iter().take_while(|b| **b != 0).map(|b| if b.is_ascii_graphic() { *b as char } else { '?' }).collect()
}
