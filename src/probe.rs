//! `freenapse probe`: read everything the mouse answers and print it raw, so
//! the owner of a mouse that is not in the device table can paste the result
//! into a GitHub issue and a table row can be written from it (CONTRIBUTING.md,
//! "Adding a mouse"). Nothing here writes to the mouse. Serial numbers are
//! masked unless asked for: they identify one unit and add nothing to a table row.

use crate::device::{DeviceSpec, Mouse};
use crate::hid::{HidError, Result};
use crate::macros;
use crate::protocol::{Report, Status, TID_DONGLE, TID_MOUSE, cmd};
use crate::sensor;

/// How many argument bytes a line shows.
pub const ARG_BYTES: usize = 16;

/// Transaction ids tried for the first exchange, most common first.
const TIDS: [u8; 3] = [0x1f, 0x3f, 0xff];

/// `1f 0b 00 ...` for the first `n` bytes.
pub fn hex(bytes: &[u8], n: usize) -> String {
    bytes.iter().take(n).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// One probe line: the label, the command it sent, and what came back. A
/// request that was not answered says so instead of aborting the probe.
pub fn line(label: &str, req: &Report, result: &Result<Report>) -> String {
    let what = format!("{label:<22} tid {:02x} {:02x}/{:02x}/{:02x}", req.transaction_id, req.command_class, req.command_id, req.data_size);
    match result {
        Ok(r) => format!("{what}  status={:02x} args={}", r.status, hex(&r.arguments, ARG_BYTES)),
        Err(e) => format!("{what}  no answer ({})", explain(e)),
    }
}

/// The status byte behind an error, with its meaning.
pub fn explain(e: &HidError) -> String {
    match e {
        HidError::Status(Status::Failure) => "status 03 failure".into(),
        HidError::Status(Status::NotSupported) => "status 05 not supported".into(),
        HidError::Status(Status::Timeout) => "status 04 timeout: mouse asleep, or that transaction id is not routed".into(),
        HidError::Status(s) => format!("status {:02x} {s:?}", *s as u8),
        HidError::BadStatus(b) => format!("status {b:02x} unknown"),
        other => other.to_string(),
    }
}

/// A line of decoded meaning under a raw line.
fn note(text: impl AsRef<str>) -> String {
    format!("{:<22} = {}", "", text.as_ref())
}

struct Probe<'a> {
    mouse: &'a Mouse,
    /// The transaction id the mouse answered the first command on.
    tid: u8,
    out: Vec<String>,
}

impl Probe<'_> {
    /// Send one read (re-addressed to the transaction id that answers), print
    /// the raw line and, when it answered, the decoded one.
    fn read(&mut self, label: &str, req: Report, decode: &dyn Fn(&Report) -> Option<String>) -> Option<Report> {
        self.read_masked(label, req, decode, false)
    }

    /// Like [`Probe::read`]; with `mask` the answer's serial number is hidden
    /// in both the raw bytes and the decoded line (see [`mask_serial`]).
    fn read_masked(&mut self, label: &str, mut req: Report, decode: &dyn Fn(&Report) -> Option<String>, mask: bool) -> Option<Report> {
        if req.transaction_id == TID_MOUSE {
            req.transaction_id = self.tid;
        }
        let r = self.mouse.raw_as_is(&req);
        let shown = if mask { r.as_ref().map(mask_serial).map_err(Clone::clone) } else { r.clone() };
        self.out.push(line(label, &req, &shown));
        if let Ok(r) = &shown {
            if let Some(d) = decode(r) {
                self.out.push(note(if mask { format!("{d}  (masked; `freenapse probe --serials` shows it)") } else { d }));
            }
        }
        r.ok()
    }

    fn blank(&mut self) {
        self.out.push(String::new());
    }
}

/// Run every read and return the report, fenced as a code block. Serial
/// numbers are masked unless `show_serials`.
pub fn run(mouse: &Mouse, show_serials: bool) -> Vec<String> {
    let spec = mouse.spec();
    let mut p = Probe { mouse, tid: spec.transaction_id, out: vec!["```text".to_string()] };
    p.out.push(format!("freenapse {} probe", env!("CARGO_PKG_VERSION")));
    p.out.push(format!("vendor id 0x{:04x}  product id 0x{:04x}  table: {}", mouse.vendor_id(), mouse.product_id(), describe_spec(spec)));
    p.out.push(format!("hid path {}", mouse.path()));
    p.blank();

    // Which transaction id the mouse answers: the table's first, then the others.
    let mut order: Vec<u8> = vec![spec.transaction_id];
    order.extend(TIDS.iter().copied().filter(|t| *t != spec.transaction_id));
    let mut answered = false;
    for t in order {
        let req = cmd::firmware_version(t);
        let r = mouse.raw_as_is(&req);
        p.out.push(line("mouse firmware", &req, &r));
        if let Ok(r) = r {
            p.out.push(note(format!("firmware {}.{}  (transaction id {t:02x} answers)", r.arguments[0], r.arguments[1])));
            p.tid = t;
            answered = true;
            break;
        }
    }
    if !answered {
        p.out.push(note("no transaction id answered: is the mouse awake? keep moving it and run the probe again"));
    }

    p.read_masked("mouse serial", cmd::serial(TID_MOUSE), &|r| Some(ascii(&r.arguments[..22])), !show_serials);
    p.read("device mode", cmd::get_device_mode(), &|r| Some(format!("mode {} ({})", r.arguments[0], if r.arguments[0] == 3 { "driver" } else { "normal" })));
    p.blank();

    p.read("dongle firmware", cmd::firmware_version(TID_DONGLE), &|r| Some(format!("firmware {}.{}", r.arguments[0], r.arguments[1])));
    p.read_masked("dongle serial", cmd::serial(TID_DONGLE), &|r| Some(ascii(&r.arguments[..22])), !show_serials);
    p.read("dongle paired list", cmd::get_paired_devices(TID_DONGLE), &|r| {
        let n = (r.arguments[0] as usize).min(8);
        let items: Vec<String> =
            (0..n).map(|i| format!("pid {:02x}{:02x} state {}", r.arguments[2 + 3 * i], r.arguments[3 + 3 * i], r.arguments[1 + 3 * i])).collect();
        Some(format!("{n} slots: {}", items.join(", ")))
    });
    p.blank();

    p.read("battery level", cmd::get_battery_level(), &|r| Some(format!("{}%", cmd::byte_to_percent(r.arguments[1]))));
    p.read("charging", cmd::get_charging_status(), &|r| Some(if r.arguments[1] != 0 { "charging".into() } else { "not charging".into() }));
    p.read("idle sleep", cmd::get_idle_time(), &|r| Some(format!("{} s", u16::from_be_bytes([r.arguments[0], r.arguments[1]]))));
    p.read("low battery", cmd::get_low_battery_threshold(), &|r| Some(format!("{}%", cmd::byte_to_percent(r.arguments[0]))));
    p.blank();

    p.read("dpi", cmd::get_dpi_xy(), &|r| {
        let a = &r.arguments;
        Some(format!("x {} y {}", u16::from_be_bytes([a[1], a[2]]), u16::from_be_bytes([a[3], a[4]])))
    });
    p.read("dpi stages", cmd::get_dpi_stages(), &|r| {
        let a = &r.arguments;
        let n = (a[2] as usize).min(5);
        let st: Vec<String> = (0..n)
            .map(|i| {
                let o = 3 + i * 7;
                format!("{}/{}", u16::from_be_bytes([a[o + 1], a[o + 2]]), u16::from_be_bytes([a[o + 3], a[o + 4]]))
            })
            .collect();
        Some(format!("active {} of {n}: {}", a[1], st.join(" ")))
    });
    p.blank();

    let hp = p.read("polling hyperpolling", cmd::get_polling_rate(), &|r| {
        let code = cmd::polling_from_code(r.arguments[1]).or_else(|| cmd::polling_from_code(r.arguments[0]));
        Some(code.map(|hz| format!("{hz} Hz")).unwrap_or_else(|| "code not in the 8000=01..125=40 table".into()))
    });
    let cl = p.read("polling classic", cmd::get_polling_rate_classic(), &|r| {
        Some(
            cmd::polling_from_code_classic(r.arguments[0])
                .map(|hz| format!("{hz} Hz"))
                .unwrap_or_else(|| "code not in the 1000=01 500=02 250=04 125=08 table".into()),
        )
    });
    p.out.push(note(match (hp.is_some(), cl.is_some()) {
        (true, true) => "both polling commands answered: the table row should say which one Synapse uses",
        (true, false) => "the HyperPolling command (00/40 set, 00/C0 get) is the one to use",
        (false, true) => "the classic command (00/05 set, 00/85 get) is the one to use",
        (false, false) => "neither polling command answered",
    }));
    p.blank();

    p.read("profile count", cmd::get_profile_count(), &|r| Some(format!("{} stored", r.arguments[0])));
    p.read("max profiles", cmd::get_max_profiles(), &|r| Some(format!("{} at most", r.arguments[0])));
    p.read("profile list", cmd::get_profile_list(), &|r| {
        let n = (r.arguments[0] as usize).min(5);
        Some(format!("ids {:?}", &r.arguments[1..1 + n]))
    });
    p.read("active profile", cmd::get_active_profile(), &|r| Some(format!("profile {}", r.arguments[0])));
    p.read("macro storage", macros::get_storage_info(), &|r| {
        let a = &r.arguments;
        let dw = |o: usize| u32::from_be_bytes([a[o], a[o + 1], a[o + 2], a[o + 3]]);
        Some(format!("ids up to {}, {} of {} bytes free", u16::from_be_bytes([a[0], a[1]]), dw(6), dw(2)))
    });
    p.blank();

    p.read("sensor state", sensor::get_state(), &|r| Some(format!("calibration data in use: {}", r.arguments[2] != 0)));
    p.read("sensor lift", sensor::get_lift(), &|r| Some(format!("lift mode {} height {}", r.arguments[2], r.arguments[3])));
    p.read("sensor config", sensor::get_config(), &|_| None);
    p.read("sensor angle", sensor::get_angle(), &|r| {
        Some(format!("rotation {}: {} deg", if r.arguments[1] != 0 { "on" } else { "off" }, r.arguments[2] as i8))
    });
    p.read("accel state", crate::accel::get_state(1), &|r| Some(format!("dynamic sensitivity {}", if r.arguments[1] != 0 { "on" } else { "off" })));
    p.read("button left", cmd::get_button_function(1, 0x01, false), &|r| Some(format!("function {}", hex(&r.arguments[3..10], 7))));

    p.out.push("```".to_string());
    p.out
}

fn describe_spec(spec: &DeviceSpec) -> String {
    if !spec.is_known() {
        "not listed (generic fallback, read-only)".into()
    } else if spec.verified {
        format!("{} (verified)", spec.name)
    } else {
        format!("{} (listed, unverified, read-only)", spec.name)
    }
}

/// The serial answer with everything after its first four characters replaced
/// by `*`, so the family prefix stays readable and the unit is not identified.
pub fn mask_serial(r: &Report) -> Report {
    let mut m = r.clone();
    for b in m.arguments[..22].iter_mut().skip(4) {
        if *b != 0 {
            *b = b'*';
        }
    }
    m
}

fn ascii(bytes: &[u8]) -> String {
    let s: String = bytes.iter().take_while(|b| **b != 0).map(|b| if b.is_ascii_graphic() { *b as char } else { '?' }).collect();
    if s.is_empty() { "(empty)".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_formatting() {
        assert_eq!(hex(&[0x1f, 0x0b, 0x00, 0xff], 16), "1f 0b 00 ff");
        assert_eq!(hex(&[0x1f, 0x0b, 0x00, 0xff], 2), "1f 0b");
        assert_eq!(hex(&[], 4), "");
        let req = cmd::get_dpi_xy();
        let mut resp = req.clone();
        resp.status = 0x02;
        resp.arguments[..5].copy_from_slice(&[0x01, 0x03, 0x20, 0x03, 0x20]);
        let l = line("dpi", &req, &Ok(resp));
        assert_eq!(l, format!("{:<22} tid 1f 04/85/07  status=02 args=01 03 20 03 20 00 00 00 00 00 00 00 00 00 00 00", "dpi"));
        let l = line("macro storage", &macros::get_storage_info(), &Err(HidError::Status(Status::NotSupported)));
        assert!(l.ends_with("06/8e/0e  no answer (status 05 not supported)"), "{l}");
        let l = line("x", &cmd::firmware_version(0xff), &Err(HidError::Status(Status::Timeout)));
        assert!(l.contains("tid ff 00/81/02  no answer (status 04 timeout"), "{l}");
        assert_eq!(explain(&HidError::BadStatus(0x77)), "status 77 unknown");
        assert_eq!(ascii(b"PM2401\0\0"), "PM2401");
        assert_eq!(ascii(&[0, 0]), "(empty)");
        assert_eq!(note("x"), format!("{:<22} = x", ""));
    }

    #[test]
    fn serial_masking() {
        let mut r = cmd::serial(TID_MOUSE);
        r.arguments[..15].copy_from_slice(b"PM2431H32400480");
        let m = mask_serial(&r);
        assert_eq!(ascii(&m.arguments[..22]), "PM24***********");
        assert_eq!(hex(&m.arguments, 6), "50 4d 32 34 2a 2a");
        assert_eq!(m.arguments[15], 0, "padding stays zero");
        assert_eq!(ascii(&r.arguments[..22]), "PM2431H32400480", "the original is untouched");
    }
}
