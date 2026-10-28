//! TOML configuration: what Synapse would store in its profile, minus the bloat.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::accel::Mode;
use crate::device::{ButtonFunction, mouse_button_name};
use crate::macros::Playback;
use crate::sensor::Tracking;

pub const DEFAULT_TOML: &str = r#"# freenapse configuration for the Razer Viper V3 Pro.
# Everything here is written to the mouse's onboard memory, so it also
# applies on other computers without any software running.

[device]
# freenapse writes only to mice an owner has verified (today: the Viper V3 Pro). Any other
# Razer mouse opens read-only: `freenapse status` and `freenapse probe` work, writes are refused.
# Set this to true to write to such a mouse anyway, at your own risk, after checking the probe
# output; please open an issue with that output so the mouse can be added to the device table.
allow_unverified = false

[dpi]
# Up to 5 stages, cycled with the button under the mouse. Range 100..35000.
stages = [400, 800, 1600, 3200, 6400]
# 1-based index of the stage to activate.
active = 2
# Optional vertical DPI per stage (independent X/Y sensitivity). Empty = same as the stage value.
y_stages = []

[polling]
# Report rate in Hz: 125, 250, 500, 1000, 2000, 4000 or 8000 (8000 needs the HyperPolling dongle).
rate = 1000
# Optional: rate to switch to while one of the listed executables is in the foreground
# (the tray app does this, like Synapse's "in-game polling rate"). Higher rates drain the battery faster.
game_rate = 8000
games = []          # e.g. ["cs2.exe", "valorant-win64-shipping.exe"]

[power]
# Seconds of inactivity before the mouse sleeps: 60..900.
idle_seconds = 300
# When wireless, the mouse enters low power mode below this battery percentage: 5..100, in steps of 5.
# The firmware only honours it at polling rates of 1000 Hz and below (Synapse greys it out above that).
low_battery_percent = 15

[sensor]
# Lift-off of the Focus Pro sensor, the two choices Synapse offers for this mouse:
#   "smart"  Smart Tracking. With asymmetric = false, lift = 1..3 mm.
#            With asymmetric = true ("Asymmetric cut-off"): lift = lift-off level 2..26,
#            land = landing level 1..25, landing below lift-off. Synapse's defaults are 13 / 12.
#   "keep"   leave whatever the mouse has stored.
tracking = "smart"
asymmetric = false
lift = 1
land = 1
# Sensor rotation in degrees (-30..30), 0 = off. Compensates a tilted grip.
rotation = 0

[acceleration]
# Dynamic Sensitivity: the mouse scales its sensitivity with the speed of the movement (onboard).
enabled = false
# "classic", "natural" or "jump" are built into the firmware; "custom" uses the curve below.
mode = "classic"
# Custom curve as [speed, multiplier] control points: speed 0..105 (the first one at 0, increasing),
# multiplier 0.1..1.5. The mouse interpolates between them like Synapse's editor; flat after the last point.
curve = [[0, 1.0], [20, 1.1], [40, 1.2], [70, 1.35], [100, 1.5]]

[dongle]
# HyperPolling dongle LED: 1 = battery status, 2 = polling rate, 3 = off.
led = 1

[profile]
# Onboard profile that the button and Dynamic Sensitivity settings go to: 1..5 on mice that
# hold several. The Viper V3 Pro reports a single profile, so it only accepts 1.
active = 1

[buttons]
# Onboard remaps for the profile above. Omitted buttons keep their current assignment.
# Values: "default", "disabled", "mouse:left|right|middle|back|forward|scroll-up|scroll-down",
#         "double-click", "turbo:left:10" (mouse button repeated N times a second while held),
#         "dpi-up", "dpi-down", "dpi-stage:N", "hypershift", "key:ctrl+shift+c", "key:f5",
#         "turbo-key:space:8" (key repeated N times a second while held),
#         "media:play-pause|mute|vol-up|vol-down|next|prev", "system:sleep|power|wake"
# Prefix with "hypershift." to bind the Hypershift layer, e.g. "hypershift.back" = "dpi-stage:5".
# back = "default"
# forward = "default"

[tray]
# How often the tray app refreshes battery while you are using the mouse, in seconds.
poll_seconds = 60
# Pop a notification when the battery drops to this percentage.
notify_percent = 15

# Per-game profiles (optional, one [[games]] block per program). While the program owns the
# foreground window the tray app applies these and restores the settings above when you leave it.
# The settings window's GAMES tab lists the games installed through Steam, Epic Games, GOG,
# EA app, Battle.net, Riot, Ubisoft Connect, Rockstar and the Xbox app and fills these in.
# [[games]]
# name = "Counter-Strike 2"  # optional: shown in the settings window
# exe = "cs2.exe"            # program file name; may be empty when dir is set
# dir = 'C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive'
#                            # optional: any program started from inside this folder counts as the game
# dpi_stage = 3          # optional: DPI stage to activate (1..5)
# polling = 8000         # optional: polling rate; omitted = polling.game_rate
# [games.buttons]        # optional: button assignments for this game, same values as [buttons]
# back = "macro:1"

# Onboard macros (optional, one [[macros]] block each), stored in the mouse's macro memory and
# bound with "macro:<id>" (play once), "macro:<id>:<n>" (n times), "macro-held:<id>",
# "macro-toggle:<id>" or "macro-seq:<id>" in [buttons]. Steps: "press:<key>", "down:<key>", "up:<key>",
# "click:<button>", "mouse-down:<button>", "mouse-up:<button>", "scroll:up|down", "delay:<ms>", "move:<dx>,<dy>".
# [[macros]]
# id = 1
# name = "Reload and jump"
# events = ["press:r", "delay:80", "press:space"]
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub device: DeviceConfig,
    pub dpi: DpiConfig,
    pub polling: PollingConfig,
    pub power: PowerConfig,
    pub sensor: SensorConfig,
    pub acceleration: AccelConfig,
    pub dongle: DongleConfig,
    pub profile: ProfileConfig,
    pub buttons: BTreeMap<String, String>,
    pub tray: TrayConfig,
    pub games: Vec<GameConfig>,
    pub macros: Vec<MacroConfig>,
}

/// Settings applied while one program is in the foreground.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct GameConfig {
    /// Display name; empty = the program name.
    pub name: String,
    /// Program file name (any case), e.g. "cs2.exe"; may be empty when `dir` is set.
    pub exe: String,
    /// Install folder: any program running from inside it matches. Empty = match `exe` only.
    pub dir: String,
    pub dpi_stage: Option<u8>,
    pub polling: Option<u16>,
    pub buttons: BTreeMap<String, String>,
}

/// An onboard macro: id, name and the steps in their text form (see [`crate::macros::parse_event`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct MacroConfig {
    pub id: u16,
    pub name: String,
    pub events: Vec<String>,
}

impl MacroConfig {
    pub fn events(&self) -> Result<Vec<crate::macros::Event>, String> {
        crate::macros::parse_events(&self.events).map_err(|e| format!("macro {} ({}): {e}", self.id, self.name))
    }

    /// The bytes the mouse stores for this macro.
    pub fn bytes(&self) -> Vec<u8> {
        crate::macros::encode(&self.events().unwrap_or_default())
    }
}

impl GameConfig {
    /// Name shown in the window and the tray tooltip.
    pub fn label(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        if !self.exe.trim().is_empty() {
            return self.exe.trim().to_string();
        }
        self.dir.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("").to_string()
    }

    /// Identifies the entry while the tray app has it applied.
    pub fn key(&self) -> String {
        format!("{}|{}", self.exe.trim().to_lowercase(), crate::library::norm_dir(&self.dir))
    }

    /// True when the program at `path` (a full image path, or a bare file name) is this game.
    pub fn matches(&self, path: &str) -> bool {
        let p = crate::library::norm_dir(path);
        let file = p.rsplit('\\').next().unwrap_or(&p);
        let exe = self.exe.trim().to_lowercase();
        if !exe.is_empty() && file == exe {
            return true;
        }
        crate::library::is_inside(&p, &crate::library::norm_dir(&self.dir))
    }
}

/// What the tray app applies while a game runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOverrides {
    /// [`GameConfig::key`] of the entry, for [`Config::game_by_key`].
    pub key: String,
    pub name: String,
    pub polling: u16,
    pub dpi_stage: Option<u8>,
    pub buttons: BTreeMap<String, String>,
}

/// Which onboard profile freenapse writes to and reads from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProfileConfig {
    pub active: u8,
}

impl Default for ProfileConfig {
    fn default() -> Self { ProfileConfig { active: 1 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DpiConfig {
    pub stages: Vec<u16>,
    pub active: u8,
    /// Vertical DPI per stage; empty means the same as `stages`.
    pub y_stages: Vec<u16>,
}

/// Dynamic Sensitivity (onboard acceleration).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AccelConfig {
    pub enabled: bool,
    pub mode: String,
    /// Custom curve control points: (speed 0..105, multiplier 0.1..1.5).
    pub curve: Vec<(u8, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PollingConfig {
    pub rate: u16,
    pub game_rate: u16,
    pub games: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PowerConfig {
    pub idle_seconds: u16,
    pub low_battery_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SensorConfig {
    pub tracking: String,
    pub asymmetric: bool,
    pub lift: u8,
    pub land: u8,
    pub rotation: i8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DongleConfig {
    pub led: u8,
}

/// Which mice freenapse may write to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct DeviceConfig {
    /// Write to a mouse that is not in the device table, or is there unverified.
    pub allow_unverified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TrayConfig {
    pub poll_seconds: u32,
    pub notify_percent: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            device: DeviceConfig::default(),
            dpi: DpiConfig::default(),
            polling: PollingConfig::default(),
            power: PowerConfig::default(),
            sensor: SensorConfig::default(),
            acceleration: AccelConfig::default(),
            dongle: DongleConfig::default(),
            profile: ProfileConfig::default(),
            buttons: BTreeMap::new(),
            tray: TrayConfig::default(),
            games: Vec::new(),
            macros: Vec::new(),
        }
    }
}

impl Default for DpiConfig {
    fn default() -> Self { DpiConfig { stages: vec![400, 800, 1600, 3200, 6400], active: 2, y_stages: vec![] } }
}
impl Default for AccelConfig {
    fn default() -> Self { AccelConfig { enabled: false, mode: "classic".into(), curve: Mode::Classic.template() } }
}

impl AccelConfig {
    pub fn mode(&self) -> Result<Mode, String> {
        Mode::from_name(&self.mode).ok_or_else(|| format!("acceleration.mode '{}' is not classic/natural/jump/custom", self.mode))
    }

    /// The 250 samples to store for the custom mode.
    pub fn samples(&self) -> [f32; crate::accel::POINTS] {
        crate::accel::curve(&self.curve)
    }
}
impl Default for PollingConfig {
    fn default() -> Self { PollingConfig { rate: 1000, game_rate: 8000, games: vec![] } }
}
impl Default for PowerConfig {
    fn default() -> Self { PowerConfig { idle_seconds: 300, low_battery_percent: 15 } }
}
impl Default for SensorConfig {
    fn default() -> Self {
        SensorConfig { tracking: "smart".into(), asymmetric: false, lift: 1, land: 1, rotation: 0 }
    }
}
impl Default for DongleConfig {
    fn default() -> Self { DongleConfig { led: 1 } }
}
impl Default for TrayConfig {
    fn default() -> Self { TrayConfig { poll_seconds: 60, notify_percent: 15 } }
}

impl Config {
    /// Load the config, creating a commented default file if none exists.
    pub fn load_or_create(path: &Path) -> Result<Config, String> {
        if !path.exists() {
            Self::write_new(path, &Config::default())?;
        }
        Self::load(path)
    }

    /// Write `cfg` as a fresh, commented config file (the default template with values substituted).
    pub fn write_new(path: &Path, cfg: &Config) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        std::fs::write(path, cfg.render()).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let cfg: Config = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// `[device] allow_unverified` of the file at `path`: false when the file is
    /// missing or unreadable. Read before a mouse is opened, so the rest of the
    /// file is not validated here.
    pub fn allow_unverified_at(path: &Path) -> bool {
        std::fs::read_to_string(path).ok().and_then(|t| toml::from_str::<Config>(&t).ok()).map(|c| c.device.allow_unverified).unwrap_or(false)
    }

    /// Validate against the Viper V3 Pro's ranges (what the file is checked
    /// against when no mouse is open). With a mouse open use [`Config::validate_for`].
    pub fn validate(&self) -> Result<(), String> {
        self.validate_for(crate::device::DeviceSpec::viper_v3_pro())
    }

    /// Validate with the DPI range and polling rates of this mouse. The in-game
    /// rates only need a code the protocol knows: they are checked against the
    /// mouse when the tray app switches to them.
    pub fn validate_for(&self, spec: &crate::device::DeviceSpec) -> Result<(), String> {
        if self.dpi.stages.is_empty() || self.dpi.stages.len() > 5 {
            return Err("dpi.stages must have 1 to 5 entries".into());
        }
        if self.dpi.active == 0 || self.dpi.active as usize > self.dpi.stages.len() {
            return Err("dpi.active must point at one of dpi.stages (1-based)".into());
        }
        if !self.dpi.y_stages.is_empty() && self.dpi.y_stages.len() != self.dpi.stages.len() {
            return Err("dpi.y_stages must be empty or have one value per stage".into());
        }
        for d in self.dpi.stages.iter().chain(&self.dpi.y_stages) {
            if !spec.dpi_ok(*d) {
                return Err(format!("dpi {d} is outside {}..{}", crate::device::DPI_MIN, spec.dpi_max));
            }
        }
        self.acceleration.mode()?;
        crate::accel::validate_points(&self.acceleration.curve).map_err(|e| format!("[acceleration] {e}"))?;
        if !spec.polling_rates.contains(&self.polling.rate) {
            return Err(format!("polling rate {} is not one of {}", self.polling.rate, spec.rates_text()));
        }
        let any_rate = crate::device::RATES_HYPERPOLLING;
        if !any_rate.contains(&self.polling.game_rate) {
            return Err(format!("polling rate {} is not one of 125/250/500/1000/2000/4000/8000", self.polling.game_rate));
        }
        if !(60..=900).contains(&self.power.idle_seconds) {
            return Err("power.idle_seconds must be 60..900".into());
        }
        if !(5..=100).contains(&self.power.low_battery_percent) {
            return Err("power.low_battery_percent must be 5..100".into());
        }
        self.tracking()?;
        if !(-30..=30).contains(&self.sensor.rotation) {
            return Err("sensor.rotation must be -30..30".into());
        }
        if !(1..=3).contains(&self.dongle.led) {
            return Err("dongle.led must be 1, 2 or 3".into());
        }
        if !(1..=5).contains(&self.profile.active) {
            return Err("profile.active must be 1..5".into());
        }
        for (k, v) in &self.buttons {
            button_target(k)?;
            parse_button(v)?;
            self.check_macro_ref(v)?;
        }
        let mut ids = Vec::new();
        for m in &self.macros {
            if m.id == 0 {
                return Err("macro ids start at 1".into());
            }
            if ids.contains(&m.id) {
                return Err(format!("macro id {} is used twice", m.id));
            }
            ids.push(m.id);
            if m.name.len() > crate::macros::NAME_MAX {
                return Err(format!("macro {} name is longer than {} bytes", m.id, crate::macros::NAME_MAX));
            }
            m.events()?;
        }
        if !self.has_left_click(None) {
            return Err("no button would be a left click: keep \"left\" at its default or assign mouse:left to another button".into());
        }
        for g in &self.games {
            if !self.has_left_click(Some(g)) {
                return Err(format!("[[games]] {}: no button would be a left click while it runs", g.label()));
            }
        }
        if self.soft_macro_slots().len() > crate::macros::MAX_SOFT_SLOTS {
            return Err(format!("at most {} different macro bindings are supported", crate::macros::MAX_SOFT_SLOTS));
        }
        for g in &self.games {
            if g.exe.trim().is_empty() && g.dir.trim().is_empty() {
                return Err("every [[games]] entry needs an exe or a dir".into());
            }
            if let Some(s) = g.dpi_stage {
                if s == 0 || s as usize > self.dpi.stages.len() {
                    return Err(format!("[[games]] {}: dpi_stage {s} is not one of the {} stages", g.label(), self.dpi.stages.len()));
                }
            }
            if let Some(p) = g.polling {
                if !any_rate.contains(&p) {
                    return Err(format!("[[games]] {}: polling rate {p} is not one of 125/250/500/1000/2000/4000/8000", g.label()));
                }
            }
            for (k, v) in &g.buttons {
                button_target(k).map_err(|e| format!("[[games]] {}: {e}", g.label()))?;
                parse_button(v).map_err(|e| format!("[[games]] {}: {e}", g.label()))?;
                self.check_macro_ref(v)?;
            }
        }
        Ok(())
    }

    /// True when some button of the normal layer (with a game's overrides on
    /// top) still clicks: a mouse with no left click cannot be used to fix it.
    pub fn has_left_click(&self, game: Option<&GameConfig>) -> bool {
        crate::device::BUTTONS.iter().any(|(name, code)| {
            let spec = game.and_then(|g| g.buttons.get(*name)).or_else(|| self.buttons.get(*name));
            match spec.map(|s| parse_button(s)) {
                None | Some(Ok(None)) => *code == 0x01,
                Some(Ok(Some(f))) => f == ButtonFunction::mouse_button(0x01),
                Some(Err(_)) => false,
            }
        })
    }

    /// A button bound to a macro must point at a macro of this config.
    fn check_macro_ref(&self, spec: &str) -> Result<(), String> {
        if let Some(Some(f)) = parse_button(spec).ok() {
            if let Some((id, _)) = f.macro_binding() {
                if !self.macros.iter().any(|m| m.id == id) {
                    return Err(format!("button assignment '{spec}' refers to macro {id}, which is not in [[macros]]"));
                }
            }
        }
        Ok(())
    }

    /// Overrides for the program in the foreground, if any. `path` is its full
    /// image path (a bare file name works too, matching by `exe` only).
    pub fn game_for(&self, path: &str) -> Option<GameOverrides> {
        if let Some(g) = self.games.iter().find(|g| g.matches(path)) {
            return Some(self.overrides(g));
        }
        let file = path.rsplit(['\\', '/']).next().unwrap_or(path).to_lowercase();
        if let Some(exe) = self.polling.games.iter().find(|g| g.to_lowercase() == file) {
            return Some(self.legacy_overrides(exe));
        }
        None
    }

    /// The overrides of the entry with this [`GameOverrides::key`].
    pub fn game_by_key(&self, key: &str) -> Option<GameOverrides> {
        if let Some(g) = self.games.iter().find(|g| g.key() == key) {
            return Some(self.overrides(g));
        }
        let exe = key.strip_prefix("polling:")?;
        self.polling.games.iter().find(|g| g.to_lowercase() == exe).map(|g| self.legacy_overrides(g))
    }

    fn overrides(&self, g: &GameConfig) -> GameOverrides {
        GameOverrides { key: g.key(), name: g.label(), polling: g.polling.unwrap_or(self.polling.game_rate), dpi_stage: g.dpi_stage, buttons: g.buttons.clone() }
    }

    /// An entry of the old `polling.games` list: in-game polling rate only.
    fn legacy_overrides(&self, exe: &str) -> GameOverrides {
        GameOverrides { key: format!("polling:{}", exe.to_lowercase()), name: exe.to_string(), polling: self.polling.game_rate, dpi_stage: None, buttons: BTreeMap::new() }
    }

    /// The macro with this id.
    pub fn macro_by_id(&self, id: u16) -> Option<&MacroConfig> {
        self.macros.iter().find(|m| m.id == id)
    }

    /// Every distinct (macro, playback) binding in the config, in a fixed order:
    /// slot i is played by the key F13 + i on mice without macro memory.
    pub fn soft_macro_slots(&self) -> Vec<(u16, Playback)> {
        let mut slots: Vec<(u16, Playback)> = Vec::new();
        let specs = self.buttons.values().chain(self.games.iter().flat_map(|g| g.buttons.values()));
        for s in specs {
            if let Some(b) = parse_button(s).ok().flatten().and_then(|f| f.macro_binding()) {
                if !slots.contains(&b) {
                    slots.push(b);
                }
            }
        }
        slots
    }

    /// The function actually written for a button: macro bindings become the
    /// slot key (F13..F24) when the mouse has no macro memory.
    pub fn resolve_button(&self, f: ButtonFunction, onboard_macros: bool) -> ButtonFunction {
        if onboard_macros {
            return f;
        }
        match f.macro_binding() {
            Some(b) => match self.soft_macro_slots().iter().position(|s| *s == b).and_then(crate::macros::soft_key_usage) {
                Some(usage) => ButtonFunction::key(0, usage),
                None => f,
            },
            None => f,
        }
    }

    /// Lowest unused macro id.
    pub fn next_macro_id(&self) -> u16 {
        (1..=u16::MAX).find(|id| !self.macros.iter().any(|m| m.id == *id)).unwrap_or(1)
    }

    /// The sensor tracking setup described by `[sensor]`, validated. `None` = "keep".
    pub fn tracking(&self) -> Result<Option<Tracking>, String> {
        let s = &self.sensor;
        let t = match (s.tracking.as_str(), s.asymmetric) {
            ("keep", _) => return Ok(None),
            ("smart", false) => Tracking::Smart { mm: s.lift },
            ("smart", true) => Tracking::Asymmetric { lift: s.lift, land: s.land },
            (other, _) => return Err(format!("sensor.tracking '{other}' is not smart/keep")),
        };
        t.plan().map_err(|e| format!("[sensor] {e}"))?;
        Ok(Some(t))
    }

    /// (X, Y) DPI per stage; Y comes from `y_stages` when it is set.
    pub fn stage_pairs(&self) -> Vec<(u16, u16)> {
        self.dpi.stages.iter().enumerate().map(|(i, d)| (*d, self.dpi.y_stages.get(i).copied().unwrap_or(*d))).collect()
    }

    /// True when any stage has a different vertical DPI.
    pub fn xy_split(&self) -> bool {
        self.dpi.y_stages.iter().zip(&self.dpi.stages).any(|(y, x)| y != x)
    }

    /// Render as the commented default template with this config's values filled in.
    /// Button remaps are appended under `[buttons]`.
    pub fn render(&self) -> String {
        let s = &self.sensor;
        let values: &[(&str, &str, String)] = &[
            ("[device]", "allow_unverified", self.device.allow_unverified.to_string()),
            ("[dpi]", "stages", format!("{:?}", self.dpi.stages)),
            ("[dpi]", "active", self.dpi.active.to_string()),
            ("[dpi]", "y_stages", format!("{:?}", self.dpi.y_stages)),
            ("[polling]", "rate", self.polling.rate.to_string()),
            ("[polling]", "game_rate", self.polling.game_rate.to_string()),
            ("[polling]", "games", format!("{:?}", self.polling.games)),
            ("[power]", "idle_seconds", self.power.idle_seconds.to_string()),
            ("[power]", "low_battery_percent", self.power.low_battery_percent.to_string()),
            ("[sensor]", "tracking", format!("{:?}", s.tracking)),
            ("[sensor]", "asymmetric", s.asymmetric.to_string()),
            ("[sensor]", "lift", s.lift.to_string()),
            ("[sensor]", "land", s.land.to_string()),
            ("[sensor]", "rotation", s.rotation.to_string()),
            ("[acceleration]", "enabled", self.acceleration.enabled.to_string()),
            ("[acceleration]", "mode", format!("{:?}", self.acceleration.mode)),
            ("[acceleration]", "curve", curve_toml(&self.acceleration.curve)),
            ("[dongle]", "led", self.dongle.led.to_string()),
            ("[profile]", "active", self.profile.active.to_string()),
            ("[tray]", "poll_seconds", self.tray.poll_seconds.to_string()),
            ("[tray]", "notify_percent", self.tray.notify_percent.to_string()),
        ];
        let mut out = String::new();
        let mut section = String::new();
        for line in DEFAULT_TOML.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                section = t.to_string();
                out.push_str(line);
                out.push('\n');
                if section == "[buttons]" {
                    continue;
                }
                continue;
            }
            let key = t.split('=').next().map(str::trim).unwrap_or("");
            if let Some((_, _, v)) = values.iter().find(|(sec, k, _)| *sec == section && *k == key) {
                let comment = line.find('#').map(|i| &line[i..]).unwrap_or("");
                out.push_str(&format!("{key} = {v}"));
                if !comment.is_empty() {
                    out.push_str(&format!("          {comment}"));
                }
                out.push('\n');
            } else {
                out.push_str(line);
                out.push('\n');
                if section == "[buttons]" && t == "# forward = \"default\"" {
                    for (k, v) in &self.buttons {
                        out.push_str(&format!("{k:?} = {v:?}\n"));
                    }
                }
            }
        }
        for g in &self.games {
            out.push_str("\n[[games]]\n");
            if !g.name.is_empty() {
                out.push_str(&format!("name = {}\n", toml_str(&g.name)));
            }
            if !g.exe.is_empty() || g.dir.is_empty() {
                out.push_str(&format!("exe = {}\n", toml_str(&g.exe)));
            }
            if !g.dir.is_empty() {
                out.push_str(&format!("dir = {}\n", toml_str(&g.dir)));
            }
            if let Some(s) = g.dpi_stage {
                out.push_str(&format!("dpi_stage = {s}\n"));
            }
            if let Some(p) = g.polling {
                out.push_str(&format!("polling = {p}\n"));
            }
            if !g.buttons.is_empty() {
                out.push_str("[games.buttons]\n");
                for (k, v) in &g.buttons {
                    out.push_str(&format!("{k:?} = {v:?}\n"));
                }
            }
        }
        for m in &self.macros {
            let events: Vec<String> = m.events.iter().map(|e| format!("{e:?}")).collect();
            out.push_str(&format!("\n[[macros]]\nid = {}\nname = {:?}\nevents = [{}]\n", m.id, m.name, events.join(", ")));
        }
        out
    }
}

/// A TOML string: a literal ('...') when possible, so Windows paths stay readable.
fn toml_str(s: &str) -> String {
    if !s.contains('\'') && !s.chars().any(char::is_control) {
        return format!("'{s}'");
    }
    toml::Value::String(s.to_string()).to_string()
}

/// `[[0, 1.0], [20, 1.1]]`
pub fn curve_toml(points: &[(u8, f32)]) -> String {
    let items: Vec<String> = points.iter().map(|(x, y)| format!("[{x}, {y:?}]")).collect();
    format!("[{}]", items.join(", "))
}

/// "back" -> (button code, hypershift=false); "hypershift.back" -> (code, true).
pub fn button_target(key: &str) -> Result<(u8, bool), String> {
    let (name, hs) = match key.strip_prefix("hypershift.") {
        Some(n) => (n, true),
        None => (key, false),
    };
    crate::device::button_code(name).map(|c| (c, hs)).ok_or_else(|| format!("unknown button '{name}'"))
}

/// Parse a button assignment. `None` means "default" (leave it alone).
pub fn parse_button(spec: &str) -> Result<Option<ButtonFunction>, String> {
    let spec = spec.trim();
    if spec == "default" {
        return Ok(None);
    }
    if spec == "disabled" {
        return Ok(Some(ButtonFunction::DISABLED));
    }
    if spec == "dpi-up" {
        return Ok(Some(ButtonFunction::dpi_cycle_up()));
    }
    if spec == "dpi-down" {
        return Ok(Some(ButtonFunction::dpi_cycle_down()));
    }
    if spec == "hypershift" {
        return Ok(Some(ButtonFunction::hypershift()));
    }
    if spec == "double-click" {
        return Ok(Some(ButtonFunction::double_click()));
    }
    if let Some(s) = spec.strip_prefix("system:") {
        let code = match s {
            "sleep" => crate::device::POWER_SLEEP,
            "power" => crate::device::POWER_OFF,
            "wake" => crate::device::POWER_WAKE,
            _ => return Err(format!("unknown system key '{s}' (sleep, power or wake)")),
        };
        return Ok(Some(ButtonFunction::power_key(code)));
    }
    if let Some(rest) = spec.strip_prefix("turbo:") {
        let (b, n) = rest.rsplit_once(':').ok_or("turbo: needs a button and a rate, e.g. turbo:left:10")?;
        let code = crate::device::button_code(b).ok_or_else(|| format!("unknown mouse button '{b}'"))?;
        return Ok(Some(ButtonFunction::turbo_button(code, turbo_rate(n)?)));
    }
    if let Some(rest) = spec.strip_prefix("turbo-key:") {
        let (k, n) = rest.rsplit_once(':').ok_or("turbo-key: needs a key and a rate, e.g. turbo-key:space:8")?;
        let Some(ButtonFunction(d)) = parse_button(&format!("key:{k}"))? else { unreachable!() };
        return Ok(Some(ButtonFunction::turbo_key(d[2], d[3], turbo_rate(n)?)));
    }
    for (prefix, play) in [("macro-held:", Playback::Held), ("macro-toggle:", Playback::Toggle), ("macro-seq:", Playback::Sequence)] {
        if let Some(id) = spec.strip_prefix(prefix) {
            return Ok(Some(ButtonFunction::macro_play(macro_id(id)?, play)));
        }
    }
    if let Some(rest) = spec.strip_prefix("macro:") {
        let (id, times) = match rest.split_once(':') {
            Some((id, n)) => {
                let n: u8 = n.trim().parse().map_err(|_| format!("bad macro repeat count '{n}'"))?;
                if n == 0 {
                    return Err("macro repeat count must be at least 1".into());
                }
                (id, n)
            }
            None => (rest, 1),
        };
        return Ok(Some(ButtonFunction::macro_play(macro_id(id)?, Playback::Times(times))));
    }
    if let Some(n) = spec.strip_prefix("dpi-stage:") {
        let n: u8 = n.parse().map_err(|_| format!("bad dpi stage '{n}'"))?;
        if !(1..=5).contains(&n) {
            return Err("dpi-stage must be 1..5".into());
        }
        return Ok(Some(ButtonFunction::dpi_stage(n)));
    }
    if let Some(b) = spec.strip_prefix("mouse:") {
        let code = crate::device::button_code(b).ok_or_else(|| format!("unknown mouse button '{b}'"))?;
        return Ok(Some(ButtonFunction::mouse_button(code)));
    }
    if let Some(m) = spec.strip_prefix("media:") {
        let usage = match m {
            "play-pause" => 0x00cd,
            "mute" => 0x00e2,
            "vol-up" => 0x00e9,
            "vol-down" => 0x00ea,
            "next" => 0x00b5,
            "prev" => 0x00b6,
            "stop" => 0x00b7,
            _ => return Err(format!("unknown media key '{m}'")),
        };
        return Ok(Some(ButtonFunction::consumer(usage)));
    }
    if let Some(k) = spec.strip_prefix("key:") {
        let mut mods = 0u8;
        let mut key = None;
        for part in k.split('+') {
            match part.trim().to_lowercase().as_str() {
                "ctrl" | "lctrl" => mods |= 0x01,
                "shift" | "lshift" => mods |= 0x02,
                "alt" | "lalt" => mods |= 0x04,
                "win" | "gui" | "lwin" => mods |= 0x08,
                "rctrl" => mods |= 0x10,
                "rshift" => mods |= 0x20,
                "ralt" | "altgr" => mods |= 0x40,
                "rwin" => mods |= 0x80,
                name => key = Some(hid_usage(name).ok_or_else(|| format!("unknown key '{name}'"))?),
            }
        }
        let key = key.ok_or("key: needs a key name, e.g. key:ctrl+c")?;
        return Ok(Some(ButtonFunction::key(mods, key)));
    }
    Err(format!("unknown button assignment '{spec}'"))
}

fn macro_id(s: &str) -> Result<u16, String> {
    let id: u16 = s.trim().parse().map_err(|_| format!("bad macro id '{s}'"))?;
    if id == 0 {
        return Err("macro ids start at 1".into());
    }
    Ok(id)
}

/// Config text of a macro binding, e.g. `macro:3`, `macro:3:5`, `macro-held:3`.
pub fn macro_spec(id: u16, play: Playback) -> String {
    match play {
        Playback::Times(1) => format!("macro:{id}"),
        Playback::Times(n) => format!("macro:{id}:{n}"),
        Playback::Held => format!("macro-held:{id}"),
        Playback::Toggle => format!("macro-toggle:{id}"),
        Playback::Sequence => format!("macro-seq:{id}"),
    }
}

fn turbo_rate(s: &str) -> Result<u8, String> {
    let n: u8 = s.trim().parse().map_err(|_| format!("bad turbo rate '{s}'"))?;
    if !(crate::device::TURBO_MIN..=crate::device::TURBO_MAX).contains(&n) {
        return Err(format!("turbo rate must be {}..{} per second", crate::device::TURBO_MIN, crate::device::TURBO_MAX));
    }
    Ok(n)
}

/// Modifier names of a key function's bitmask, in config order.
pub fn modifier_names(mask: u8) -> Vec<String> {
    let mut parts = Vec::new();
    for (bit, name) in [(0x01, "ctrl"), (0x02, "shift"), (0x04, "alt"), (0x08, "win"), (0x10, "rctrl"), (0x20, "rshift"), (0x40, "ralt"), (0x80, "rwin")] {
        if mask & bit != 0 {
            parts.push(name.to_string());
        }
    }
    parts
}

/// Keyboard HID usage ids (usage page 0x07).
pub fn hid_usage(name: &str) -> Option<u8> {
    let b = name.as_bytes();
    if b.len() == 1 {
        return match b[0] {
            c @ b'a'..=b'z' => Some(0x04 + (c - b'a')),
            b'1'..=b'9' => Some(0x1e + (b[0] - b'1')),
            b'0' => Some(0x27),
            b'-' => Some(0x2d),
            b'=' => Some(0x2e),
            b'[' => Some(0x2f),
            b']' => Some(0x30),
            b'\\' => Some(0x31),
            b';' => Some(0x33),
            b'\'' => Some(0x34),
            b'`' => Some(0x35),
            b',' => Some(0x36),
            b'.' => Some(0x37),
            b'/' => Some(0x38),
            _ => None,
        };
    }
    if let Some(n) = name.strip_prefix('f') {
        if let Ok(n) = n.parse::<u8>() {
            if (1..=12).contains(&n) {
                return Some(0x3a + n - 1);
            }
            if (13..=24).contains(&n) {
                return Some(0x68 + n - 13);
            }
        }
    }
    Some(match name {
        "enter" | "return" => 0x28,
        "esc" | "escape" => 0x29,
        "backspace" => 0x2a,
        "tab" => 0x2b,
        "space" => 0x2c,
        "capslock" => 0x39,
        "printscreen" => 0x46,
        "scrolllock" => 0x47,
        "pause" => 0x48,
        "insert" => 0x49,
        "home" => 0x4a,
        "pageup" => 0x4b,
        "delete" | "del" => 0x4c,
        "end" => 0x4d,
        "pagedown" => 0x4e,
        "right" => 0x4f,
        "left" => 0x50,
        "down" => 0x51,
        "up" => 0x52,
        "numlock" => 0x53,
        _ => return None,
    })
}

/// Inverse of [`parse_button`] for the assignments the config can express.
pub fn spec_for(f: &ButtonFunction) -> Option<String> {
    let d = &f.0;
    Some(match d[0] {
        0x00 => "disabled".to_string(),
        0x01 => {
            let name = crate::device::BUTTONS.iter().find(|(_, c)| *c == d[2])?.0;
            format!("mouse:{name}")
        }
        0x02 => {
            let mut parts = modifier_names(d[2]);
            parts.push(usage_name(d[3])?);
            format!("key:{}", parts.join("+"))
        }
        0x0d => {
            let mut parts = modifier_names(d[2]);
            parts.push(usage_name(d[3])?);
            format!("turbo-key:{}:{}", parts.join("+"), f.turbo_rate()?)
        }
        0x0e => {
            let name = crate::device::BUTTONS.iter().find(|(_, c)| *c == d[2])?.0;
            format!("turbo:{name}:{}", f.turbo_rate()?)
        }
        0x0b => "double-click".to_string(),
        0x09 => match d[2] {
            crate::device::POWER_SLEEP => "system:sleep".to_string(),
            crate::device::POWER_OFF => "system:power".to_string(),
            crate::device::POWER_WAKE => "system:wake".to_string(),
            _ => return None,
        },
        0x0a => {
            let name = match u16::from_be_bytes([d[2], d[3]]) {
                0x00cd => "play-pause",
                0x00e2 => "mute",
                0x00e9 => "vol-up",
                0x00ea => "vol-down",
                0x00b5 => "next",
                0x00b6 => "prev",
                0x00b7 => "stop",
                _ => return None,
            };
            format!("media:{name}")
        }
        0x06 => match (d[2], d[3]) {
            (0x01, _) => "dpi-up".to_string(),
            (0x02, _) => "dpi-down".to_string(),
            (0x03, n) if (1..=5).contains(&n) => format!("dpi-stage:{n}"),
            _ => return None,
        },
        0x0c => "hypershift".to_string(),
        0x03..=0x05 | 0x0f => {
            let (id, play) = f.macro_binding()?;
            macro_spec(id, play)
        }
        _ => return None,
    })
}

/// Key name for a keyboard HID usage, the inverse of [`hid_usage`].
pub fn usage_name(usage: u8) -> Option<String> {
    const NAMED: [&str; 23] = [
        "enter", "esc", "backspace", "tab", "space", "capslock", "printscreen", "scrolllock", "pause", "insert", "home",
        "pageup", "delete", "end", "pagedown", "right", "left", "down", "up", "numlock", "-", "=", "[",
    ];
    let mut candidates: Vec<String> = (b'a'..=b'z').map(|c| (c as char).to_string()).collect();
    candidates.extend((b'0'..=b'9').map(|c| (c as char).to_string()));
    candidates.extend((1..=24).map(|n| format!("f{n}")));
    candidates.extend(NAMED.iter().map(|s| s.to_string()));
    candidates.extend(["]", "\\", ";", "'", "`", ",", ".", "/"].iter().map(|s| s.to_string()));
    candidates.into_iter().find(|n| hid_usage(n) == Some(usage))
}

/// Human-readable description of a button function, for `freenapse buttons`.
pub fn describe(f: &ButtonFunction) -> String {
    let d = &f.0;
    match d[0] {
        0x01 => format!("mouse:{}", mouse_button_name(d[2])),
        _ => f.describe(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let c = Config::default();
        c.validate().unwrap();
        assert_eq!(c.dpi.stages.len(), 5);
        assert_eq!(c.polling.rate, 1000);
        assert!(!c.device.allow_unverified);
    }

    #[test]
    fn device_section() {
        let parsed: Config = toml::from_str("[device]\nallow_unverified = true\n").unwrap();
        assert!(parsed.device.allow_unverified);
        let text = parsed.render();
        assert!(text.contains("allow_unverified = true"), "{text}");
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, parsed);
        // The flag is read without the rest of the file having to validate.
        let dir = std::env::temp_dir().join(format!("freenapse-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        std::fs::write(&p, "[device]\nallow_unverified = true\n[dpi]\nstages = []\n").unwrap();
        assert!(Config::allow_unverified_at(&p));
        assert!(Config::load(&p).is_err());
        std::fs::write(&p, "[dpi]\nstages = [800]\nactive = 1\n").unwrap();
        assert!(!Config::allow_unverified_at(&p));
        assert!(!Config::allow_unverified_at(&dir.join("missing.toml")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_against_a_spec() {
        use crate::device::DeviceSpec;
        let mut c = Config::default();
        c.dpi.stages = vec![400, 35000];
        c.dpi.active = 1;
        c.validate().unwrap();
        let wired = DeviceSpec::lookup(0x00C0);
        c.validate_for(&wired).unwrap();
        // A smaller sensor refuses the top stage, with its own maximum in the message.
        let small = DeviceSpec::lookup(0x00A6);
        let e = c.validate_for(&small).unwrap_err();
        assert!(e.contains("35000") && e.contains("30000"), "{e}");
        // The classic polling command has no 2000 Hz; the in-game rate is not held to the mouse's list.
        c.dpi.stages = vec![400];
        c.polling.rate = 2000;
        c.validate().unwrap();
        let e = c.validate_for(&wired).unwrap_err();
        assert!(e.contains("2000") && e.contains("125/500/1000"), "{e}");
        c.polling.rate = 1000;
        c.polling.game_rate = 8000;
        c.validate_for(&wired).unwrap();
        c.polling.game_rate = 3000;
        assert!(c.validate_for(&wired).is_err());
        // The unknown fallback is held to its own, conservative ranges.
        c.polling.game_rate = 8000;
        c.dpi.stages = vec![30001];
        assert!(c.validate_for(&DeviceSpec::unknown(0x0123)).is_err());
    }

    #[test]
    fn default_toml_matches_defaults() {
        let parsed: Config = toml::from_str(DEFAULT_TOML).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let parsed: Config = toml::from_str("[dpi]\nstages = [1600]\nactive = 1\n").unwrap();
        assert_eq!(parsed.dpi.stages, vec![1600]);
        assert_eq!(parsed.power, PowerConfig::default());
        parsed.validate().unwrap();
    }

    #[test]
    fn button_specs() {
        assert_eq!(parse_button("default").unwrap(), None);
        assert_eq!(parse_button("key:ctrl+c").unwrap(), Some(ButtonFunction([0x02, 2, 0x01, 0x06, 0, 0, 0])));
        assert_eq!(parse_button("dpi-stage:3").unwrap(), Some(ButtonFunction([0x06, 2, 0x03, 3, 0, 0, 0])));
        assert_eq!(parse_button("mouse:middle").unwrap(), Some(ButtonFunction::mouse_button(3)));
        assert!(parse_button("key:bogus").is_err());
        assert_eq!(button_target("hypershift.back").unwrap(), (0x04, true));
    }

    #[test]
    fn spec_roundtrip() {
        for spec in [
            "disabled", "mouse:back", "key:ctrl+shift+f5", "key:space", "media:mute", "dpi-up", "dpi-stage:4", "hypershift",
            "double-click", "turbo:left:10", "turbo-key:ctrl+c:8", "system:sleep", "system:power", "system:wake",
        ] {
            let f = parse_button(spec).unwrap().unwrap();
            assert_eq!(spec_for(&f).as_deref(), Some(spec), "{spec}");
        }
        assert_eq!(parse_button("turbo:left:10").unwrap(), Some(ButtonFunction([0x0e, 3, 0x01, 0, 100, 0, 0])));
        assert_eq!(parse_button("turbo-key:space:8").unwrap(), Some(ButtonFunction([0x0d, 4, 0, 0x2c, 0, 125, 0])));
        assert!(parse_button("turbo:left:0").is_err());
        assert!(parse_button("system:reboot").is_err());
    }

    #[test]
    fn render_roundtrip() {
        let mut c = Config::default();
        c.dpi.stages = vec![800, 1600];
        c.dpi.active = 2;
        c.dpi.y_stages = vec![800, 2000];
        c.acceleration.enabled = true;
        c.acceleration.mode = "custom".into();
        c.acceleration.curve = vec![(0, 0.5), (30, 1.0), (105, 1.5)];
        c.profile.active = 3;
        c.polling.games = vec!["cs2.exe".into()];
        c.sensor.tracking = "smart".into();
        c.sensor.asymmetric = true;
        c.sensor.lift = 14;
        c.sensor.land = 12;
        c.sensor.rotation = -5;
        c.buttons.insert("back".into(), "dpi-up".into());
        c.buttons.insert("hypershift.forward".into(), "key:ctrl+c".into());
        c.buttons.insert("middle".into(), "macro:2:3".into());
        c.macros.push(MacroConfig { id: 2, name: "Reload \"fast\"".into(), events: vec!["press:r".into(), "delay:80".into()] });
        let mut g = GameConfig { exe: "cs2.exe".into(), dpi_stage: Some(1), polling: Some(4000), ..Default::default() };
        g.buttons.insert("back".into(), "macro-held:2".into());
        c.games.push(g);
        c.games.push(GameConfig { exe: "valorant.exe".into(), ..Default::default() });
        c.games.push(GameConfig { name: "Rocket League®".into(), dir: "C:\\Steam\\steamapps\\common\\rocketleague".into(), polling: Some(2000), ..Default::default() });
        c.games.push(GameConfig { name: "It's \"quoted\"".into(), exe: "q.exe".into(), ..Default::default() });
        let text = c.render();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, c);
        back.validate().unwrap();
        assert_eq!(c.game_for("CS2.EXE").unwrap().polling, 4000);
        assert_eq!(c.game_for("valorant.exe").unwrap().polling, 8000);
        assert!(c.game_for("notepad.exe").is_none());
        assert_eq!(c.game_for("C:\\Games\\CS2\\bin\\win64\\cs2.exe").unwrap().polling, 4000);
        let rl = c.game_for("c:\\steam\\steamapps\\common\\RocketLeague\\Binaries\\Win64\\RocketLeague.exe").unwrap();
        assert_eq!((rl.polling, rl.name.as_str()), (2000, "Rocket League®"));
        assert_eq!(c.game_by_key(&rl.key).unwrap().polling, 2000);
        assert!(c.game_for("C:\\Steam\\steamapps\\common\\rocketleague2\\x.exe").is_none());
        c.polling.games = vec!["Old.exe".into()];
        assert_eq!(c.game_by_key(&c.game_for("old.exe").unwrap().key).unwrap().polling, 8000);
        assert_eq!(c.next_macro_id(), 1);
    }

    #[test]
    fn macro_specs() {
        for (spec, f) in [
            ("macro:3", ButtonFunction([0x03, 3, 0, 3, 1, 0, 0])),
            ("macro:3:5", ButtonFunction([0x03, 3, 0, 3, 5, 0, 0])),
            ("macro-held:300", ButtonFunction([0x04, 2, 1, 44, 0, 0, 0])),
            ("macro-toggle:3", ButtonFunction([0x05, 2, 0, 3, 0, 0, 0])),
            ("macro-seq:3", ButtonFunction([0x0f, 2, 0, 3, 0, 0, 0])),
        ] {
            assert_eq!(parse_button(spec).unwrap(), Some(f), "{spec}");
            assert_eq!(spec_for(&f).as_deref(), Some(spec), "{spec}");
        }
        assert!(parse_button("macro:0").is_err());
        assert!(parse_button("macro:1:0").is_err());
        let mut c = Config::default();
        c.buttons.insert("back".into(), "macro:1".into());
        assert!(c.validate().is_err(), "macro reference without a macro");
        c.macros.push(MacroConfig { id: 1, name: "x".into(), events: vec!["press:a".into()] });
        c.validate().unwrap();
        c.macros.push(MacroConfig { id: 1, name: "y".into(), events: vec![] });
        assert!(c.validate().is_err(), "duplicate id");
    }

    #[test]
    fn left_click_is_protected() {
        let mut c = Config::default();
        c.buttons.insert("left".into(), "key:a".into());
        assert!(c.validate().is_err(), "left click lost");
        c.buttons.insert("back".into(), "mouse:left".into());
        c.validate().unwrap();
        c.buttons.insert("hypershift.left".into(), "key:b".into());
        c.validate().unwrap();
        let mut g = GameConfig { exe: "x.exe".into(), ..Default::default() };
        g.buttons.insert("back".into(), "key:c".into());
        c.games.push(g);
        assert!(c.validate().is_err(), "game layer loses the only left click");
    }

    #[test]
    fn soft_slots() {
        let mut c = Config::default();
        c.macros.push(MacroConfig { id: 1, name: "a".into(), events: vec!["press:a".into()] });
        c.macros.push(MacroConfig { id: 2, name: "b".into(), events: vec!["press:b".into()] });
        c.buttons.insert("back".into(), "macro:2".into());
        c.buttons.insert("forward".into(), "macro-held:1".into());
        c.buttons.insert("hypershift.back".into(), "macro:2".into());
        assert_eq!(c.soft_macro_slots(), vec![(2, Playback::Times(1)), (1, Playback::Held)]);
        let f = parse_button("macro-held:1").unwrap().unwrap();
        assert_eq!(c.resolve_button(f, false), ButtonFunction::key(0, 0x69));
        assert_eq!(c.resolve_button(f, true), f);
        let plain = parse_button("mouse:left").unwrap().unwrap();
        assert_eq!(c.resolve_button(plain, false), plain);
    }

    #[test]
    fn rejects_bad_values() {
        let mut c = Config::default();
        c.polling.rate = 3000;
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.dpi.active = 9;
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.dpi.y_stages = vec![800];
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.acceleration.mode = "fast".into();
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.acceleration.curve = vec![(0, 1.0), (200, 1.0)];
        assert!(c.validate().is_err());
    }

    #[test]
    fn xy_stages() {
        let mut c = Config::default();
        c.dpi.stages = vec![800, 1600];
        assert_eq!(c.stage_pairs(), vec![(800, 800), (1600, 1600)]);
        assert!(!c.xy_split());
        c.dpi.y_stages = vec![800, 1200];
        assert_eq!(c.stage_pairs(), vec![(800, 800), (1600, 1200)]);
        assert!(c.xy_split());
    }
}
