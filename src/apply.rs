//! Push a [`Config`] to the mouse. Every setting is read first and only written
//! when it differs, so repeated runs do not wear the onboard flash.

use crate::config::{Config, button_target, parse_button, spec_for};
use crate::device::{BUTTONS, ButtonFunction, Mouse};
use crate::hid::{HidError, Result};
use crate::sensor;

/// Build a config from what the mouse currently has stored, so the first run
/// keeps the settings Synapse left behind instead of replacing them.
pub fn import(mouse: &Mouse) -> Result<Config> {
    let mut cfg = Config::default();

    let st = mouse.dpi_stages()?;
    if !st.stages.is_empty() {
        cfg.dpi.stages = st.stages.iter().map(|s| s.0).collect();
        cfg.dpi.active = st.active.clamp(1, st.stages.len() as u8);
        if st.stages.iter().any(|(x, y)| x != y) {
            cfg.dpi.y_stages = st.stages.iter().map(|s| s.1).collect();
        }
    }
    if let Ok((on, mode)) = mouse.acceleration() {
        cfg.acceleration.enabled = on;
        cfg.acceleration.mode = mode.name().into();
        // The stored custom curve cannot be turned back into control points; the
        // default template stays in the config and is written on the next apply.
    }
    if let Ok(p) = mouse.polling_rate() {
        cfg.polling.rate = p;
    }
    if let Ok(s) = mouse.idle_time() {
        cfg.power.idle_seconds = s.clamp(60, 900);
    }
    if let Ok(p) = mouse.low_battery_threshold() {
        cfg.power.low_battery_percent = p.clamp(5, 100);
    }

    // Sensor: Smart Tracking can be read back; the asymmetric levels cannot, so
    // that mode is kept as is until the user picks values in the window. On a
    // mouse whose sensor page is not known, a failed read means "keep" too.
    let s = match mouse.sensor() {
        Ok(s) => Some(s),
        Err(e) if mouse.spec().sensor_tracking => return Err(e),
        Err(_) => None,
    };
    let sc = &mut cfg.sensor;
    sc.tracking = "keep".into();
    if let Some(s) = s {
        if s.mode == sensor::LIFT_MODE_SMART && !s.use_calibration {
            sc.tracking = "smart".into();
            sc.lift = (s.height + 1).clamp(1, 3);
        }
    }
    if let Ok((on, deg)) = mouse.rotation() {
        sc.rotation = if on { deg.clamp(-30, 30) } else { 0 };
    }

    if mouse.spec().pairing {
        if let Ok(led) = mouse.dongle_led_mode() {
            if (1..=3).contains(&led) {
                cfg.dongle.led = led;
            }
        }
    }

    cfg.profile.active = mouse.profile();
    // Macros already in the mouse (Synapse's or ours): keep them so their bindings stay valid.
    if let Ok(ids) = mouse.macro_ids().and_then(|ids| if mouse.macro_support()? { Ok(ids) } else { Ok(Vec::new()) }) {
        for id in ids {
            let Ok(data) = mouse.macro_data(id) else { continue };
            let name = mouse.macro_name(id).ok().filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Macro {id}"));
            let events = crate::macros::event_specs(&crate::macros::decode(&data));
            cfg.macros.push(crate::config::MacroConfig { id, name, events });
        }
    }
    for (name, code) in BUTTONS {
        for hs in [false, true] {
            let Ok(f) = mouse.button(mouse.profile(), code, hs) else { continue };
            if f == ButtonFunction::mouse_button(code) {
                continue;
            }
            if let Some(spec) = spec_for(&f) {
                let key = if hs { format!("hypershift.{name}") } else { name.to_string() };
                cfg.buttons.insert(key, spec);
            }
        }
    }

    if cfg.validate().is_err() {
        // Something odd on the mouse: keep the parts that validate on their own.
        let mut safe = Config::default();
        safe.dpi = cfg.dpi.clone();
        safe.polling.rate = cfg.polling.rate;
        if safe.validate().is_err() {
            safe = Config::default();
        }
        return Ok(safe);
    }
    Ok(cfg)
}

/// Whether the mouse's acceleration state differs from the config. A custom
/// curve is checked on its first and last packets only (reading all 32 takes
/// seconds through the dongle).
fn accel_differs(mouse: &Mouse, cfg: &Config) -> Result<bool> {
    let (on, mode) = mouse.acceleration()?;
    let a = &cfg.acceleration;
    if on != a.enabled {
        return Ok(true);
    }
    if !on {
        return Ok(false);
    }
    let want = a.mode().map_err(|_| HidError::Unsupported("invalid [acceleration] section"))?;
    if mode != want {
        return Ok(true);
    }
    if want == crate::accel::Mode::Custom {
        let samples = a.samples();
        for offset in [0usize, crate::accel::POINTS - crate::accel::CHUNK] {
            let stored = mouse.acceleration_curve_chunk(offset as u8, crate::accel::CHUNK as u8)?;
            if stored != samples[offset..offset + crate::accel::CHUNK] {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Apply the whole configuration. `polling_rate` lets the caller pick between
/// the normal and the in-game rate. Returns a line per change made.
pub fn apply(mouse: &Mouse, cfg: &Config, polling_rate: u16) -> Result<Vec<String>> {
    let mut changes = Vec::new();
    let spec = mouse.spec();
    cfg.validate_for(spec).map_err(HidError::Invalid)?;

    // Everything profile-bound below (buttons, acceleration) goes to this profile.
    if mouse.profile() != cfg.profile.active {
        mouse.select_profile(cfg.profile.active)?;
        changes.push(format!("onboard profile -> {}", cfg.profile.active));
    }

    let want_stages = cfg.stage_pairs();
    let cur = mouse.dpi_stages()?;
    if cur.stages != want_stages || cur.active != cfg.dpi.active {
        mouse.set_dpi_stages(cfg.dpi.active, &want_stages)?;
        changes.push(format!("dpi stages -> {:?} (active {})", cfg.dpi.stages, cfg.dpi.active));
    } else {
        let want = want_stages[cfg.dpi.active as usize - 1];
        if mouse.dpi()? != want {
            mouse.set_dpi(want.0, want.1)?;
            changes.push(format!("dpi -> {}", want.0));
        }
    }

    if mouse.polling_rate().ok() != Some(polling_rate) {
        mouse.set_polling_rate(polling_rate)?;
        changes.push(format!("polling -> {polling_rate} Hz"));
    }

    if mouse.idle_time()? != cfg.power.idle_seconds {
        mouse.set_idle_time(cfg.power.idle_seconds)?;
        changes.push(format!("idle sleep -> {} s", cfg.power.idle_seconds));
    }

    if mouse.low_battery_threshold()? != cfg.power.low_battery_percent {
        mouse.set_low_battery_threshold(cfg.power.low_battery_percent)?;
        changes.push(format!("low battery -> {}%", cfg.power.low_battery_percent));
    }

    let tracking = cfg.tracking().map_err(|_| HidError::Unsupported("invalid [sensor] section"))?;
    // The tracking page is only written where it is known to apply (src/sensor.rs).
    if let Some(tracking) = tracking.filter(|_| spec.sensor_tracking) {
        let plan = tracking.plan().map_err(|_| HidError::Unsupported("invalid [sensor] section"))?;
        let cur = mouse.sensor()?;
        // The asymmetric levels cannot be read back, so that mode is always rewritten.
        let differs = cur.use_calibration != plan.state || (cur.mode, cur.height) != plan.lift || plan.config.is_some();
        if differs {
            mouse.apply_tracking(&plan)?;
            changes.push(format!("tracking -> {tracking:?}"));
        }
    }

    if spec.supports_acceleration && accel_differs(mouse, cfg)? {
        let mode = cfg.acceleration.mode().map_err(|_| HidError::Unsupported("invalid [acceleration] section"))?;
        let samples = cfg.acceleration.samples();
        mouse.set_acceleration(cfg.acceleration.enabled, mode, Some(&samples))?;
        changes.push(if cfg.acceleration.enabled { format!("dynamic sensitivity -> {}", mode.name()) } else { "dynamic sensitivity -> off".into() });
    }

    let want_rot = cfg.sensor.rotation;
    if spec.supports_rotation {
        match mouse.rotation() {
            Ok((rot_on, rot_deg)) => {
                if rot_on != (want_rot != 0) || (rot_on && rot_deg != want_rot) {
                    mouse.set_rotation(want_rot)?;
                    changes.push(format!("rotation -> {want_rot} deg"));
                }
            }
            Err(e) if spec.sensor_tracking => return Err(e),
            // Angle tune is part of the sensor page: left alone where that page is unknown.
            Err(_) => {}
        }
    }

    if spec.pairing && mouse.dongle_led_mode().ok() != Some(cfg.dongle.led) {
        mouse.set_dongle_led_mode(cfg.dongle.led)?;
        changes.push(format!("dongle led -> {}", cfg.dongle.led));
    }

    changes.extend(apply_macros(mouse, cfg)?);

    // Every button of both layers: a key missing from [buttons] means its default function.
    let onboard = mouse.macro_support()?;
    for (name, code) in BUTTONS {
        for hs in [false, true] {
            let key = if hs { format!("hypershift.{name}") } else { name.to_string() };
            let spec = cfg.buttons.get(&key);
            let func = match spec.map(|s| parse_button(s)) {
                Some(Ok(Some(f))) => cfg.resolve_button(f, onboard),
                _ => ButtonFunction::mouse_button(code),
            };
            if mouse.button(mouse.profile(), code, hs)? != func {
                mouse.set_button(mouse.profile(), code, hs, func)?;
                changes.push(format!("button {key} -> {}", spec.map(String::as_str).unwrap_or("default")));
            }
        }
    }
    let _ = button_target;

    Ok(changes)
}

/// The function to write for a config assignment on this mouse.
pub fn resolve(mouse: &Mouse, cfg: &Config, f: ButtonFunction) -> Result<ButtonFunction> {
    Ok(cfg.resolve_button(f, mouse.macro_support()?))
}

/// Store every configured macro whose bytes differ from what the mouse holds.
/// Macros in the mouse that the config does not list are left alone. Mice
/// without macro memory get nothing: the tray app plays their macros.
pub fn apply_macros(mouse: &Mouse, cfg: &Config) -> Result<Vec<String>> {
    let mut changes = Vec::new();
    if cfg.macros.is_empty() || !mouse.macro_support()? {
        return Ok(changes);
    }
    let stored = mouse.macro_ids()?;
    for m in &cfg.macros {
        let bytes = m.bytes();
        let same = stored.contains(&m.id) && mouse.macro_data(m.id).map(|d| d == bytes).unwrap_or(false);
        if !same {
            mouse.write_macro(m.id, &m.name, &bytes)?;
            changes.push(format!("macro {} ({}) -> {} bytes", m.id, m.name, bytes.len()));
        }
    }
    Ok(changes)
}

/// Write the button layout of a game (or the base layout when `game` is None)
/// for the normal layer. Returns true when anything was written.
pub fn apply_game_buttons(mouse: &Mouse, cfg: &Config, game: Option<&crate::config::GameOverrides>) -> Result<bool> {
    let mut written = false;
    let onboard = mouse.macro_support()?;
    for (name, code) in BUTTONS {
        let spec = game.and_then(|g| g.buttons.get(name)).or_else(|| cfg.buttons.get(name));
        let func = match spec.map(|s| parse_button(s)) {
            Some(Ok(Some(f))) => cfg.resolve_button(f, onboard),
            _ => ButtonFunction::mouse_button(code),
        };
        if mouse.button(mouse.profile(), code, false)? != func {
            mouse.set_button(mouse.profile(), code, false, func)?;
            written = true;
        }
    }
    Ok(written)
}
