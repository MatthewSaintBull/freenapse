//! Onboard macros: the macro memory commands (class 0x06) and the event byte
//! format, as Synapse 4 writes them to mice that have macro memory.
//!
//! A macro is stored in two parts, both addressed by a 16-bit id:
//! - a 250-byte name structure (GUID, item count, UTF-8 name, timestamp, hash),
//!   written with `0x04` in chunks of 64 bytes;
//! - the event bytes, allocated with `0x08` and written with `0x09` in chunks
//!   of 72 bytes.
//!
//! Event bytes: `01 key` make, `02 key` break (keyboard
//! HID usage), `08 mask` mouse button state, `0a 01|ff` wheel up/down,
//! `11 ms` / `12 ms16` / `13 ms24` / `14 ms32` delays, `15 x16 y16` cursor move.

use crate::protocol::{Report, TID_MOUSE};

pub const CLASS: u8 = 0x06;

/// Size of the name structure Synapse writes for every macro.
pub const NAME_STRUCT_LEN: usize = 250;
/// Bytes of event data per `set_data_chunk` packet.
pub const DATA_CHUNK: usize = 72;
/// Bytes of the name structure per `set_name_chunk` packet.
pub const NAME_CHUNK: usize = 64;
/// Longest macro name, in UTF-8 bytes.
pub const NAME_MAX: usize = 100;
/// Playback repeat count range for "N times".
pub const TIMES_MAX: u8 = 255;

/// Software playback (mice without macro memory): a bound macro gets one of
/// the keys F13..F24 in the mouse; the tray app plays it when that key arrives.
pub const SOFT_KEY_BASE: u8 = 0x68; // HID usage of F13
pub const MAX_SOFT_SLOTS: usize = 12;

pub fn soft_key_usage(slot: usize) -> Option<u8> {
    (slot < MAX_SOFT_SLOTS).then(|| SOFT_KEY_BASE + slot as u8)
}

pub fn soft_slot(usage: u8) -> Option<usize> {
    (SOFT_KEY_BASE..SOFT_KEY_BASE + MAX_SOFT_SLOTS as u8).contains(&usage).then(|| (usage - SOFT_KEY_BASE) as usize)
}

pub const EV_KEY_DOWN: u8 = 0x01;
pub const EV_KEY_UP: u8 = 0x02;
pub const EV_BUTTONS: u8 = 0x08;
pub const EV_WHEEL: u8 = 0x0a;
pub const EV_DELAY_1: u8 = 0x11;
pub const EV_DELAY_2: u8 = 0x12;
pub const EV_DELAY_3: u8 = 0x13;
pub const EV_DELAY_4: u8 = 0x14;
pub const EV_CURSOR: u8 = 0x15;

/// Keyboard usages of the modifier keys (usage page 7, 0xe0..0xe7).
pub const MODIFIER_USAGES: [(&str, u8); 8] = [
    ("ctrl", 0xe0),
    ("shift", 0xe1),
    ("alt", 0xe2),
    ("win", 0xe3),
    ("rctrl", 0xe4),
    ("rshift", 0xe5),
    ("ralt", 0xe6),
    ("rwin", 0xe7),
];

/// One recorded step of a macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Keyboard key by HID usage (modifiers 0xe0..0xe7).
    KeyDown(u8),
    KeyUp(u8),
    /// Mouse button 1..5 (left, right, middle, back, forward).
    ButtonDown(u8),
    ButtonUp(u8),
    /// +1 = wheel up, -1 = wheel down.
    Scroll(i8),
    /// Pause in milliseconds.
    Delay(u32),
    /// Relative cursor move.
    Cursor(i16, i16),
    /// Bytes we could not decode (kept verbatim so re-writing is lossless).
    Raw(Vec<u8>),
}

/// How a button plays the macro (Synapse's playback options).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    /// Play it `n` times per press (Synapse: "Play once" is n = 1).
    Times(u8),
    /// Repeat while the button is held.
    Held,
    /// Press to start repeating, press again to stop.
    Toggle,
    /// Each press plays the next step of the macro.
    Sequence,
}

impl Playback {
    pub fn label(self) -> String {
        match self {
            Playback::Times(1) => "Play once".into(),
            Playback::Times(n) => format!("Play {n} times"),
            Playback::Held => "While held".into(),
            Playback::Toggle => "Toggle".into(),
            Playback::Sequence => "Sequence".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Event text form, used by the config file: "press:a", "delay:50", ...
// ---------------------------------------------------------------------------

/// Keyboard usage of a key name: the names of `config::hid_usage` plus the modifiers.
pub fn key_usage(name: &str) -> Option<u8> {
    let n = name.trim().to_lowercase();
    MODIFIER_USAGES.iter().find(|(k, _)| *k == n).map(|(_, u)| *u).or_else(|| crate::config::hid_usage(&n))
}

/// Inverse of [`key_usage`].
pub fn key_name(usage: u8) -> Option<String> {
    MODIFIER_USAGES.iter().find(|(_, u)| *u == usage).map(|(k, _)| k.to_string()).or_else(|| crate::config::usage_name(usage))
}

fn button_code(name: &str) -> Result<u8, String> {
    match name {
        "left" => Ok(1),
        "right" => Ok(2),
        "middle" => Ok(3),
        "back" => Ok(4),
        "forward" => Ok(5),
        _ => Err(format!("unknown mouse button '{name}' (left, right, middle, back, forward)")),
    }
}

pub fn button_name(code: u8) -> &'static str {
    match code {
        1 => "left",
        2 => "right",
        3 => "middle",
        4 => "back",
        5 => "forward",
        _ => "?",
    }
}

/// Parse one config step. `press:x` and `click:x` expand to a down and an up.
pub fn parse_event(spec: &str) -> Result<Vec<Event>, String> {
    let spec = spec.trim();
    let (kind, arg) = spec.split_once(':').ok_or_else(|| format!("macro step '{spec}' needs a value, e.g. press:a or delay:50"))?;
    let arg = arg.trim();
    let key = || key_usage(arg).ok_or_else(|| format!("unknown key '{arg}'"));
    Ok(match kind.trim() {
        "down" => vec![Event::KeyDown(key()?)],
        "up" => vec![Event::KeyUp(key()?)],
        "press" => {
            let k = key()?;
            vec![Event::KeyDown(k), Event::KeyUp(k)]
        }
        "mouse-down" => vec![Event::ButtonDown(button_code(arg)?)],
        "mouse-up" => vec![Event::ButtonUp(button_code(arg)?)],
        "click" => {
            let b = button_code(arg)?;
            vec![Event::ButtonDown(b), Event::ButtonUp(b)]
        }
        "scroll" => vec![Event::Scroll(match arg {
            "up" => 1,
            "down" => -1,
            _ => return Err(format!("scroll must be up or down, not '{arg}'")),
        })],
        "delay" => {
            let ms: u32 = arg.parse().map_err(|_| format!("bad delay '{arg}' (milliseconds)"))?;
            vec![Event::Delay(ms)]
        }
        "move" => {
            let (x, y) = arg.split_once(',').ok_or("move needs dx,dy")?;
            let x: i16 = x.trim().parse().map_err(|_| format!("bad move '{arg}'"))?;
            let y: i16 = y.trim().parse().map_err(|_| format!("bad move '{arg}'"))?;
            vec![Event::Cursor(x, y)]
        }
        "raw" => {
            let bytes: Result<Vec<u8>, _> = (0..arg.len() / 2).map(|i| u8::from_str_radix(&arg[2 * i..2 * i + 2], 16)).collect();
            vec![Event::Raw(bytes.map_err(|_| format!("bad raw bytes '{arg}'"))?)]
        }
        other => return Err(format!("unknown macro step '{other}' (down, up, press, mouse-down, mouse-up, click, scroll, delay, move)")),
    })
}

/// Parse a whole step list.
pub fn parse_events(specs: &[String]) -> Result<Vec<Event>, String> {
    let mut out = Vec::new();
    for s in specs {
        out.extend(parse_event(s)?);
    }
    Ok(out)
}

/// Config text of one event (`press` is never produced: down and up stay separate).
pub fn event_spec(e: &Event) -> String {
    match e {
        Event::KeyDown(u) => format!("down:{}", key_name(*u).unwrap_or_else(|| format!("0x{u:02x}"))),
        Event::KeyUp(u) => format!("up:{}", key_name(*u).unwrap_or_else(|| format!("0x{u:02x}"))),
        Event::ButtonDown(b) => format!("mouse-down:{}", button_name(*b)),
        Event::ButtonUp(b) => format!("mouse-up:{}", button_name(*b)),
        Event::Scroll(d) => format!("scroll:{}", if *d > 0 { "up" } else { "down" }),
        Event::Delay(ms) => format!("delay:{ms}"),
        Event::Cursor(x, y) => format!("move:{x},{y}"),
        Event::Raw(b) => format!("raw:{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
    }
}

/// Merge consecutive down/up pairs into `press:` / `click:` for readability.
pub fn event_specs(events: &[Event]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < events.len() {
        match (&events[i], events.get(i + 1)) {
            (Event::KeyDown(a), Some(Event::KeyUp(b))) if a == b => {
                out.push(format!("press:{}", key_name(*a).unwrap_or_else(|| format!("0x{a:02x}"))));
                i += 2;
            }
            (Event::ButtonDown(a), Some(Event::ButtonUp(b))) if a == b => {
                out.push(format!("click:{}", button_name(*a)));
                i += 2;
            }
            (e, _) => {
                out.push(event_spec(e));
                i += 1;
            }
        }
    }
    out
}

/// Short human summary, e.g. "Ctrl + C, 50 ms, Space".
pub fn describe(events: &[Event]) -> String {
    let specs = event_specs(events);
    let parts: Vec<String> = specs
        .iter()
        .map(|s| match s.split_once(':') {
            Some(("press", k)) => cap(k),
            Some(("down", k)) => format!("{} down", cap(k)),
            Some(("up", k)) => format!("{} up", cap(k)),
            Some(("click", b)) => format!("{} click", cap(b)),
            Some(("mouse-down", b)) => format!("{} down", cap(b)),
            Some(("mouse-up", b)) => format!("{} up", cap(b)),
            Some(("scroll", d)) => format!("scroll {d}"),
            Some(("delay", ms)) => format!("{ms} ms"),
            Some(("move", v)) => format!("move {v}"),
            _ => s.clone(),
        })
        .collect();
    parts.join(", ")
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) if s.len() == 1 => f.to_uppercase().collect(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Byte encoding
// ---------------------------------------------------------------------------

/// Encode events into Synapse's byte form.
pub fn encode(events: &[Event]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut mask = 0u8;
    for e in events {
        match e {
            Event::KeyDown(u) => out.extend_from_slice(&[EV_KEY_DOWN, *u]),
            Event::KeyUp(u) => out.extend_from_slice(&[EV_KEY_UP, *u]),
            Event::ButtonDown(b) => {
                mask |= 1 << (b.clamp(&1, &5) - 1);
                out.extend_from_slice(&[EV_BUTTONS, mask]);
            }
            Event::ButtonUp(b) => {
                mask &= !(1 << (b.clamp(&1, &5) - 1));
                out.extend_from_slice(&[EV_BUTTONS, mask]);
            }
            Event::Scroll(d) => out.extend_from_slice(&[EV_WHEEL, if *d > 0 { 1 } else { 0xff }]),
            Event::Delay(ms) => {
                let b = ms.to_be_bytes();
                if *ms < 256 {
                    out.extend_from_slice(&[EV_DELAY_1, b[3]]);
                } else if *ms < 65536 {
                    out.extend_from_slice(&[EV_DELAY_2, b[2], b[3]]);
                } else if *ms < 1 << 24 {
                    out.extend_from_slice(&[EV_DELAY_3, b[1], b[2], b[3]]);
                } else {
                    out.extend_from_slice(&[EV_DELAY_4, b[0], b[1], b[2], b[3]]);
                }
            }
            Event::Cursor(x, y) => {
                let (x, y) = (x.to_be_bytes(), y.to_be_bytes());
                out.extend_from_slice(&[EV_CURSOR, x[0], x[1], y[0], y[1]]);
            }
            Event::Raw(b) => out.extend_from_slice(b),
        }
    }
    out
}

/// Decode stored bytes. Unknown bytes end up in a single `Raw` event.
pub fn decode(bytes: &[u8]) -> Vec<Event> {
    let mut out = Vec::new();
    let mut mask = 0u8;
    let mut i = 0;
    while i < bytes.len() {
        let need = |n: usize| i + n <= bytes.len();
        match bytes[i] {
            EV_KEY_DOWN if need(2) => {
                out.push(Event::KeyDown(bytes[i + 1]));
                i += 2;
            }
            EV_KEY_UP if need(2) => {
                out.push(Event::KeyUp(bytes[i + 1]));
                i += 2;
            }
            EV_BUTTONS if need(2) => {
                let new = bytes[i + 1];
                for b in 0..5u8 {
                    let bit = 1 << b;
                    if new & bit != 0 && mask & bit == 0 {
                        out.push(Event::ButtonDown(b + 1));
                    }
                    if new & bit == 0 && mask & bit != 0 {
                        out.push(Event::ButtonUp(b + 1));
                    }
                }
                mask = new;
                i += 2;
            }
            EV_WHEEL if need(2) => {
                out.push(Event::Scroll(if bytes[i + 1] == 1 { 1 } else { -1 }));
                i += 2;
            }
            EV_DELAY_1 if need(2) => {
                out.push(Event::Delay(bytes[i + 1] as u32));
                i += 2;
            }
            EV_DELAY_2 if need(3) => {
                out.push(Event::Delay(u16::from_be_bytes([bytes[i + 1], bytes[i + 2]]) as u32));
                i += 3;
            }
            EV_DELAY_3 if need(4) => {
                out.push(Event::Delay(u32::from_be_bytes([0, bytes[i + 1], bytes[i + 2], bytes[i + 3]])));
                i += 4;
            }
            EV_DELAY_4 if need(5) => {
                out.push(Event::Delay(u32::from_be_bytes([bytes[i + 1], bytes[i + 2], bytes[i + 3], bytes[i + 4]])));
                i += 5;
            }
            EV_CURSOR if need(5) => {
                out.push(Event::Cursor(i16::from_be_bytes([bytes[i + 1], bytes[i + 2]]), i16::from_be_bytes([bytes[i + 3], bytes[i + 4]])));
                i += 5;
            }
            _ => {
                out.push(Event::Raw(bytes[i..].to_vec()));
                break;
            }
        }
    }
    out
}

/// Synapse rounds the memory a macro needs up to whole 256-byte blocks, plus one block.
pub fn required_memory(data_len: usize) -> u32 {
    (((data_len + 256) as u32).div_ceil(256)) * 256
}

/// The 250-byte name structure: GUID, item count (little endian), name, timestamp, hash.
pub fn name_structure(id: u16, name: &str, items: u32) -> [u8; NAME_STRUCT_LEN] {
    let mut s = [0u8; NAME_STRUCT_LEN];
    // A GUID that identifies the macro as freenapse's: "fn" + id in the first bytes.
    s[0] = b'f';
    s[1] = b'n';
    s[2..4].copy_from_slice(&id.to_be_bytes());
    s[4..16].copy_from_slice(b"freenapse\0\0\0");
    s[16..20].copy_from_slice(&items.to_le_bytes());
    let n = name.as_bytes();
    let len = utf8_prefix(name, NAME_MAX);
    s[20..20 + len].copy_from_slice(&n[..len]);
    s
}

/// Longest UTF-8 prefix of `s` that fits in `max` bytes.
pub fn utf8_prefix(s: &str, max: usize) -> usize {
    let mut len = s.len().min(max);
    while !s.is_char_boundary(len) {
        len -= 1;
    }
    len
}

/// Name stored in a name structure (or in its first 120 bytes).
pub fn name_from_structure(s: &[u8]) -> String {
    let n = s.get(20..20 + NAME_MAX).unwrap_or(&[]);
    let end = n.iter().position(|b| *b == 0).unwrap_or(n.len());
    String::from_utf8_lossy(&n[..end]).into_owned()
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Storage info: [maxId u16, maxMemory u32, available u32, recyclable u32].
pub fn get_storage_info() -> Report { Report::new(TID_MOUSE, CLASS, 0x8e, 0x0e) }
/// Number of stored macros (u16).
pub fn get_count() -> Report { Report::new(TID_MOUSE, CLASS, 0x80, 0x02) }
/// Ids of the stored macros: [offset u16, count u16, id u16...].
pub fn get_id_list() -> Report { Report::new(TID_MOUSE, CLASS, 0x81, 0x50) }
pub fn create(id: u16) -> Report { Report::new(TID_MOUSE, CLASS, 0x02, 0x02).with_args(&id.to_be_bytes()) }
pub fn delete(id: u16) -> Report { Report::new(TID_MOUSE, CLASS, 0x03, 0x02).with_args(&id.to_be_bytes()) }
/// Reserve `size` bytes of event data for the macro.
pub fn allocate(id: u16, size: u32) -> Report {
    let mut a = id.to_be_bytes().to_vec();
    a.extend_from_slice(&size.to_be_bytes());
    Report::new(TID_MOUSE, CLASS, 0x08, 0x06).with_args(&a)
}
/// Stored size of the macro's event data: [id u16, size u32].
pub fn get_data_size(id: u16) -> Report { Report::new(TID_MOUSE, CLASS, 0x88, 0x06).with_args(&id.to_be_bytes()) }
/// One chunk of the name structure: [id, offset u16, total u16, bytes...].
pub fn set_name_chunk(id: u16, offset: u16, total: u16, chunk: &[u8]) -> Report {
    let mut a = id.to_be_bytes().to_vec();
    a.extend_from_slice(&offset.to_be_bytes());
    a.extend_from_slice(&total.to_be_bytes());
    a.extend_from_slice(chunk);
    Report::new(TID_MOUSE, CLASS, 0x04, (6 + chunk.len()) as u8).with_args(&a)
}
/// Read the name structure from `offset`: [id, offset u16] -> [id, offset, size u16, bytes...].
pub fn get_name_chunk(id: u16, offset: u16) -> Report {
    let mut a = id.to_be_bytes().to_vec();
    a.extend_from_slice(&offset.to_be_bytes());
    Report::new(TID_MOUSE, CLASS, 0x84, 0x50).with_args(&a)
}
/// One chunk of event data: [id, offset u32, count, bytes...].
pub fn set_data_chunk(id: u16, offset: u32, chunk: &[u8]) -> Report {
    let mut a = id.to_be_bytes().to_vec();
    a.extend_from_slice(&offset.to_be_bytes());
    a.push(chunk.len() as u8);
    a.extend_from_slice(chunk);
    Report::new(TID_MOUSE, CLASS, 0x09, (7 + chunk.len()) as u8).with_args(&a)
}
/// Read `count` bytes of event data from `offset`: -> [id, offset u32, count, bytes...].
pub fn get_data_chunk(id: u16, offset: u32, count: u8) -> Report {
    let mut a = id.to_be_bytes().to_vec();
    a.extend_from_slice(&offset.to_be_bytes());
    a.push(count);
    Report::new(TID_MOUSE, CLASS, 0x89, 0x50).with_args(&a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_encode() {
        let ev = parse_events(&["press:a".into(), "delay:50".into(), "delay:300".into(), "click:left".into(), "scroll:down".into(), "down:ctrl".into(), "up:ctrl".into(), "move:-3,4".into()].to_vec()).unwrap();
        let bytes = encode(&ev);
        assert_eq!(
            bytes,
            vec![
                0x01, 0x04, 0x02, 0x04, 0x11, 50, 0x12, 0x01, 0x2c, 0x08, 0x01, 0x08, 0x00, 0x0a, 0xff, 0x01, 0xe0, 0x02, 0xe0, 0x15, 0xff, 0xfd, 0x00, 0x04
            ]
        );
        assert_eq!(decode(&bytes), ev);
        assert_eq!(event_specs(&ev), ["press:a", "delay:50", "delay:300", "click:left", "scroll:down", "press:ctrl", "move:-3,4"]);
        assert_eq!(describe(&ev[..4]), "A, 50 ms, 300 ms");
    }

    #[test]
    fn button_masks_track_state() {
        let ev = vec![Event::ButtonDown(1), Event::ButtonDown(2), Event::ButtonUp(1), Event::ButtonUp(2)];
        assert_eq!(encode(&ev), vec![0x08, 0x01, 0x08, 0x03, 0x08, 0x02, 0x08, 0x00]);
        assert_eq!(decode(&encode(&ev)), ev);
    }

    #[test]
    fn long_delays_and_raw() {
        let ev = vec![Event::Delay(70000), Event::Delay(20_000_000)];
        let b = encode(&ev);
        assert_eq!(b, vec![0x13, 0x01, 0x11, 0x70, 0x14, 0x01, 0x31, 0x2d, 0x00]);
        assert_eq!(decode(&b), ev);
        let raw = decode(&[0x01, 0x04, 0x77, 0x01]);
        assert_eq!(raw, vec![Event::KeyDown(4), Event::Raw(vec![0x77, 0x01])]);
        assert_eq!(encode(&raw), vec![0x01, 0x04, 0x77, 0x01]);
        assert_eq!(parse_event("raw:7701").unwrap(), vec![Event::Raw(vec![0x77, 0x01])]);
        assert!(parse_event("bogus:1").is_err());
        assert!(parse_event("press:nokey").is_err());
    }

    #[test]
    fn name_structure_layout() {
        let s = name_structure(7, "Reload", 3);
        assert_eq!(&s[..2], b"fn");
        assert_eq!(&s[2..4], &[0, 7]);
        assert_eq!(&s[16..20], &[3, 0, 0, 0]);
        assert_eq!(&s[20..26], b"Reload");
        assert_eq!(name_from_structure(&s), "Reload");
        assert_eq!(required_memory(10), 512);
        assert_eq!(required_memory(0), 256);
        assert_eq!(required_memory(256), 512);
        assert_eq!(required_memory(257), 768);
    }

    #[test]
    fn command_layouts() {
        let r = set_data_chunk(0x1234, 72, &[1, 2, 3]);
        assert_eq!(r.data_size, 10);
        assert_eq!(&r.arguments[..10], &[0x12, 0x34, 0, 0, 0, 72, 3, 1, 2, 3]);
        let r = set_name_chunk(1, 64, 250, &[9; 64]);
        assert_eq!(r.data_size, 70);
        assert_eq!(&r.arguments[..6], &[0, 1, 0, 64, 0, 250]);
        let r = allocate(1, 300);
        assert_eq!(&r.arguments[..6], &[0, 1, 0, 0, 1, 44]);
    }
}
