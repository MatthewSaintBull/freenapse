//! `freenapse.exe`: launched without arguments it is the tray app (no console
//! window); with a command it is the CLI, printing to the console it was
//! started from.

#![windows_subsystem = "windows"]

use std::process::ExitCode;

use freenapse::config::{Config, describe};
use freenapse::device::{self, Mouse};
use freenapse::protocol::{Report, TID_MOUSE};
use freenapse::sensor::Tracking;
use freenapse::{apply, config_path};

const USAGE: &str = "\
freenapse - lightweight Razer Viper V3 Pro control (replaces Razer Synapse)

The wireless mouse only answers while it is awake: keep moving it while running commands.

USAGE:
  freenapse status                       battery, DPI, polling, sleep, sensor, dongle
  freenapse apply                        write the config file to the mouse (onboard memory)
  freenapse config                       print the config file path (created from the mouse if missing)
  freenapse import                       overwrite the config file with the mouse's current settings
  freenapse dpi <x> [y]                  set the live DPI (100..35000)
  freenapse stages <active> <dpi>...     set up to 5 DPI stages, e.g. stages 2 400 800 1600
  freenapse polling <hz>                 125|250|500|1000|2000|4000|8000
  freenapse idle <seconds>               sleep after N seconds idle (60..900)
  freenapse low-battery <percent>        low power mode threshold (5..100, honoured at <= 1000 Hz)
  freenapse lift smart <mm>              smart tracking, 1..3 mm
  freenapse lift asym <liftoff> <landing>    asymmetric cut-off: lift-off 2..26, landing 1..25 (Synapse default 13 12)
  freenapse rotation <degrees>           sensor rotation -30..30, 0 = off
  freenapse dynamic off|classic|natural|jump|custom   Dynamic Sensitivity (custom uses the config's curve)
  freenapse dongle-led <1|2|3>           HyperPolling dongle LED: battery | polling rate | off
  freenapse buttons [profile]            show the onboard button mapping (default profile 1)
  freenapse profiles                     onboard profiles: how many exist, how many the mouse can hold
  freenapse profile <1..5>               select the onboard profile (created if the mouse has room)
  freenapse macros                       macros stored in the mouse and the macro memory left
  freenapse macro-read <id>              print a stored macro's steps in config form
  freenapse macro-write <id>             store the macro with this id from the config file
  freenapse macro-delete <id>            remove a macro from the mouse
  freenapse pair-status                  devices the dongle is paired with, auto-pairing state
  freenapse pair [pid]                   put the dongle in pairing mode (default: this mouse's wireless id)
  freenapse pair-cancel [pid]            leave pairing mode
  freenapse unpair --yes [pid]           forget the pairing (the mouse stops working until paired again)
  freenapse games                        installed games found in Steam, Epic, GOG, EA, Battle.net, Riot, Ubisoft, Xbox
  freenapse autostart on|off             run the tray app at login
  freenapse ui                           open the settings window
  freenapse raw <class> <id> <size> [args..]   send a raw command (hex bytes)
  freenapse sensor-dump                  raw sensor registers
  freenapse watch [n]                    read the battery once a second

Run freenapse.exe without arguments for the tray icon (battery, DPI stage and polling switch,
per-game profiles, macro playback).
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let first = args.first().map(String::as_str);
    if first.is_none() || first == Some("--tray") || first == Some("--settings") {
        // Tray mode: no console. A second instance opens the settings window of the running one.
        freenapse::tray::run(args.iter().any(|a| a == "--settings"));
        return ExitCode::SUCCESS;
    }
    // CLI mode: this is a windowed executable, so attach to the console we were started from.
    unsafe {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
    let Some(cmd) = args.first() else {
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match run(cmd, &args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: &str, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        "config" => {
            let p = config_path();
            if !p.exists() {
                match Mouse::open().and_then(|m| apply::import(&m)) {
                    Ok(cfg) => {
                        Config::write_new(&p, &cfg)?;
                        println!("created from the mouse's current settings");
                    }
                    Err(e) => {
                        Config::write_new(&p, &Config::default())?;
                        println!("created with defaults ({e})");
                    }
                }
            }
            println!("{}", p.display());
            return Ok(());
        }
        "ui" => {
            // The window lives in the tray process: start one (or ask the running one) with --settings.
            let exe = std::env::current_exe()?;
            std::process::Command::new(&exe).arg("--settings").spawn().map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
            return Ok(());
        }
        "autostart" => {
            let on = match args.first().map(String::as_str) {
                Some("on") => true,
                Some("off") => false,
                _ => return Err("autostart on|off".into()),
            };
            freenapse::autostart::set(on)?;
            println!("autostart {}", if on { "enabled" } else { "disabled" });
            return Ok(());
        }
        "games" => {
            // Installed games found in the launchers, and which ones have a profile.
            let cfg = Config::load(&config_path()).unwrap_or_default();
            let games = freenapse::library::scan();
            for g in &games {
                let profile = cfg.games.iter().find(|p| p.matches(&format!("{}\\{}", g.dir, if g.exe.is_empty() { "?" } else { &g.exe })));
                let mark = profile.map(|p| format!("  [profile: {}]", p.label())).unwrap_or_default();
                println!("{:<16} {}{mark}\n                 {}", g.source, g.name, g.dir);
            }
            println!("{} games", games.len());
            return Ok(());
        }
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            return Ok(());
        }
        _ => {}
    }

    let mouse = Mouse::open()?;
    match cmd {
        "status" => status(&mouse)?,
        "import" => {
            let cfg = apply::import(&mouse)?;
            let p = config_path();
            Config::write_new(&p, &cfg)?;
            println!("wrote {}", p.display());
            print!("{}", cfg.render());
        }
        "apply" => {
            let cfg = Config::load_or_create(&config_path())?;
            let changes = apply::apply(&mouse, &cfg, cfg.polling.rate)?;
            if changes.is_empty() {
                println!("mouse already matches the config");
            } else {
                for c in changes {
                    println!("{c}");
                }
            }
        }
        "dpi" => {
            let x: u16 = arg(args, 0, "dpi x")?;
            let y: u16 = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(x);
            mouse.set_dpi(x, y)?;
            println!("dpi set to {:?}", mouse.dpi()?);
        }
        "stages" => {
            let active: u8 = arg(args, 0, "active stage")?;
            let stages: Vec<(u16, u16)> =
                args[1..].iter().map(|s| s.parse::<u16>().map(|d| (d, d))).collect::<Result<_, _>>()?;
            if stages.is_empty() || stages.len() > 5 {
                return Err("give 1 to 5 DPI values".into());
            }
            mouse.set_dpi_stages(active, &stages)?;
            println!("{:?}", mouse.dpi_stages()?);
        }
        "polling" => {
            let hz: u16 = arg(args, 0, "polling rate")?;
            mouse.set_polling_rate(hz)?;
            println!("polling rate {} Hz", mouse.polling_rate()?);
        }
        "idle" => {
            let s: u16 = arg(args, 0, "seconds")?;
            mouse.set_idle_time(s)?;
            println!("idle timeout {} s", mouse.idle_time()?);
        }
        "low-battery" => {
            let p: u8 = arg(args, 0, "percent")?;
            mouse.set_low_battery_threshold(p)?;
            println!("low battery threshold {}%", mouse.low_battery_threshold()?);
        }
        "lift" => {
            let kind = args.first().map(String::as_str).ok_or("missing mode")?;
            let v1: u8 = arg(args, 1, "value")?;
            let v2: u8 = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(1);
            let t = match kind {
                "smart" => Tracking::Smart { mm: v1 },
                "asym" | "smart-asym" => Tracking::Asymmetric { lift: v1, land: v2 },
                _ => return Err("lift mode must be smart <mm> or asym <liftoff> <landing>".into()),
            };
            let plan = t.plan()?;
            mouse.apply_tracking(&plan)?;
            println!("sensor: {}", mouse.sensor()?.describe());
        }
        "rotation" => {
            let deg: i8 = arg(args, 0, "degrees")?;
            if !(-30..=30).contains(&deg) {
                return Err("rotation must be -30..30".into());
            }
            mouse.set_rotation(deg)?;
            let (on, d) = mouse.rotation()?;
            println!("rotation {}", if on { format!("{d} deg") } else { "off".into() });
        }
        "profiles" => {
            let (n, max) = mouse.profiles()?;
            println!("onboard profiles: {n} stored, the mouse can hold {max}: ids {:?}", mouse.profile_list()?);
            match mouse.active_profile() {
                Ok(a) => println!("active: {a}"),
                Err(e) => println!("active: not reported by this mouse ({e}); with one profile it is 1"),
            }
        }
        "profile" => {
            let id: u8 = arg(args, 0, "profile id")?;
            mouse.select_profile(id).map_err(|e| {
                let (n, max) = mouse.profiles().unwrap_or((1, 1));
                format!("{e} (the mouse holds {n} of at most {max} profiles)")
            })?;
            println!("onboard profile {id} selected; set profile.active = {id} in the config to keep using it");
        }
        "macros" => {
            let st = mouse.macro_storage()?;
            println!("macro memory: {} of {} bytes free ({} recyclable), ids up to {}", st.available, st.total, st.recyclable, st.max_id);
            let ids = mouse.macro_ids()?;
            if ids.is_empty() {
                println!("no macros stored");
            }
            for id in ids {
                let name = mouse.macro_name(id).unwrap_or_default();
                let size = mouse.macro_data_size(id).map(|s| s.to_string()).unwrap_or_else(|e| e.to_string());
                println!("{id:>5}  {size:>6} bytes  {name}");
            }
        }
        "macro-read" => {
            let id: u16 = arg(args, 0, "macro id")?;
            let data = mouse.macro_data(id)?;
            let events = freenapse::macros::decode(&data);
            println!("name = {:?}", mouse.macro_name(id).unwrap_or_default());
            println!("events = {:?}", freenapse::macros::event_specs(&events));
            println!("bytes = {data:02x?}");
        }
        "macro-write" => {
            let id: u16 = arg(args, 0, "macro id")?;
            let cfg = Config::load_or_create(&config_path())?;
            let m = cfg.macro_by_id(id).ok_or_else(|| format!("macro {id} is not in the config file"))?;
            let bytes = m.bytes();
            mouse.write_macro(id, &m.name, &bytes)?;
            let back = mouse.macro_data(id)?;
            println!("macro {id} ({}) stored: {} bytes, read back {}", m.name, bytes.len(), if back == bytes { "identical" } else { "DIFFERENT" });
        }
        "macro-delete" => {
            let id: u16 = arg(args, 0, "macro id")?;
            mouse.delete_macro(id)?;
            println!("macro {id} deleted; remaining ids {:?}", mouse.macro_ids()?);
        }
        "pair-status" => {
            for d in mouse.paired_devices()? {
                println!("paired: product id 0x{:04x}, link state {}", d.product_id, d.status);
            }
            match mouse.auto_pairing() {
                Ok(on) => println!("auto pairing: {}", if on { "on" } else { "off" }),
                Err(e) => println!("auto pairing: {e}"),
            }
        }
        "pair" | "pair-cancel" | "unpair" => {
            let yes = args.iter().any(|a| a == "--yes");
            let pid = args.iter().find(|a| !a.starts_with("--")).map(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16)).transpose()?.unwrap_or(device::PID_VIPER_V3_PRO_WIRELESS);
            match cmd {
                "pair" => {
                    let mode = mouse.start_pairing(pid)?;
                    println!("dongle in pairing mode for product id 0x{pid:04x} (answer {mode}); switch the mouse off and on next to the dongle");
                }
                "pair-cancel" => {
                    mouse.cancel_pairing(pid)?;
                    println!("pairing cancelled");
                }
                _ => {
                    if !yes {
                        return Err("unpair removes the mouse from the dongle; add --yes to confirm".into());
                    }
                    mouse.unpair(pid)?;
                    println!("product id 0x{pid:04x} unpaired; run `freenapse pair` to pair it again");
                }
            }
        }
        "dynamic" => {
            let what = args.first().map(String::as_str).ok_or("dynamic off|classic|natural|jump|custom")?;
            if what == "off" {
                mouse.set_acceleration(false, freenapse::accel::Mode::Classic, None)?;
            } else {
                let mode = freenapse::accel::Mode::from_name(what).ok_or("dynamic off|classic|natural|jump|custom")?;
                let cfg = Config::load_or_create(&config_path())?;
                let samples = cfg.acceleration.samples();
                mouse.set_acceleration(true, mode, Some(&samples))?;
            }
            let (on, mode) = mouse.acceleration()?;
            println!("dynamic sensitivity: {}", if on { mode.name() } else { "off" });
        }
        "dongle-led" => {
            let m: u8 = arg(args, 0, "mode")?;
            mouse.set_dongle_led_mode(m)?;
            println!("dongle led mode {}", mouse.dongle_led_mode()?);
        }
        "buttons" => {
            let profile: u8 = args.first().map(|s| s.parse()).transpose()?.unwrap_or(1);
            for (name, code) in device::BUTTONS {
                for hs in [false, true] {
                    let layer = if hs { "hypershift" } else { "" };
                    match mouse.button(profile, code, hs) {
                        Ok(f) => println!("{name:<12}{layer:<12}{}", describe(&f)),
                        Err(e) => println!("{name:<12}{layer:<12}error: {e}"),
                    }
                }
            }
        }
        "raw" => {
            let (tid, args) = match args.first().map(String::as_str) {
                Some("-d") | Some("--dongle") => (freenapse::protocol::TID_DONGLE, &args[1..]),
                _ => (TID_MOUSE, args),
            };
            let bytes: Vec<u8> = args
                .iter()
                .map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16))
                .collect::<Result<_, _>>()?;
            if bytes.len() < 3 {
                return Err("raw [-d] <class> <id> <size> [args..]".into());
            }
            let req = Report::new(tid, bytes[0], bytes[1], bytes[2]).with_args(&bytes[3..]);
            let r = mouse.raw(&req)?;
            let n = (r.data_size as usize).max(8).min(80);
            println!("status={:02x} size={} args={:02x?}", r.status, r.data_size, &r.arguments[..n]);
        }
        "sensor-dump" => {
            for (name, v) in mouse.sensor_dump() {
                match v {
                    Ok(b) => println!("{name}: {b:02x?}"),
                    Err(e) => println!("{name}: error {e}"),
                }
            }
        }
        "watch" => {
            let n: u32 = args.first().map(|s| s.parse()).transpose()?.unwrap_or(30);
            for _ in 0..n {
                let t = std::time::Instant::now();
                let r = mouse.battery();
                println!("{:?} in {:?}  (user idle {} ms)", r.map(|b| b.percent), t.elapsed(), freenapse::idle_ms());
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        }
        _ => {
            eprint!("{USAGE}");
            return Err(format!("unknown command {cmd}").into());
        }
    }
    Ok(())
}

fn arg<T: std::str::FromStr>(args: &[String], i: usize, what: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    let s = args.get(i).ok_or_else(|| format!("missing {what}"))?;
    s.parse().map_err(|e| format!("bad {what} '{s}': {e}"))
}

fn status(mouse: &Mouse) -> Result<(), Box<dyn std::error::Error>> {
    println!("connection:   {}", if mouse.is_wireless() { "wireless (HyperSpeed dongle)" } else { "wired" });
    if mouse.is_wireless() {
        if let Ok((a, b)) = mouse.dongle_firmware() {
            println!("dongle:       fw {a}.{b}  serial {}", mouse.dongle_serial().unwrap_or_default());
        }
    }
    match mouse.firmware() {
        Ok((a, b)) => println!("mouse:        fw {a}.{b}  serial {}", mouse.serial().unwrap_or_default()),
        Err(e) => {
            println!("mouse:        {e}");
            return Ok(());
        }
    }
    let b = mouse.battery()?;
    println!("battery:      {}%{}", b.percent, if b.charging { " (charging)" } else { "" });
    println!("dpi:          {:?}", mouse.dpi()?);
    let st = mouse.dpi_stages()?;
    println!("dpi stages:   active {} of {:?}", st.active, st.stages.iter().map(|s| s.0).collect::<Vec<_>>());
    println!("polling:      {}", mouse.polling_rate().map(|p| format!("{p} Hz")).unwrap_or_else(|e| e.to_string()));
    println!("idle sleep:   {}", mouse.idle_time().map(|p| format!("{p} s")).unwrap_or_else(|e| e.to_string()));
    println!("low battery:  {}", mouse.low_battery_threshold().map(|p| format!("{p}%")).unwrap_or_else(|e| e.to_string()));
    match mouse.sensor() {
        Ok(s) => println!("sensor:       {}", s.describe()),
        Err(e) => println!("sensor:       {e}"),
    }
    match mouse.rotation() {
        Ok((true, d)) => println!("rotation:     {d} deg"),
        Ok((false, _)) => println!("rotation:     off"),
        Err(e) => println!("rotation:     {e}"),
    }
    match mouse.acceleration() {
        Ok((true, m)) => println!("dynamic sens: {}", m.name()),
        Ok((false, _)) => println!("dynamic sens: off"),
        Err(e) => println!("dynamic sens: {e}"),
    }
    if let Ok((n, max)) = mouse.profiles() {
        println!("profiles:     {n} onboard (the mouse supports {max}), using profile {}", mouse.profile());
    }
    if mouse.is_wireless() {
        println!("dongle led:   {}", mouse.dongle_led_mode().map(|p| p.to_string()).unwrap_or_else(|e| e.to_string()));
    }
    Ok(())
}

