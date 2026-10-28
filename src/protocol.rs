//! Razer USB HID control protocol: 90-byte feature reports.
//!
//! Layout (offsets inside the 90-byte report):
//! ```text
//!  0      status          0x00 on request; 0x01 busy, 0x02 ok, 0x03 failure, 0x04 timeout, 0x05 unsupported
//!  1      transaction id  routes the packet: 0x1f = mouse (through the dongle), 0xff = the dongle itself
//!  2..4   remaining packets (big endian, unused here)
//!  4      protocol type   0x00
//!  5      data size       number of meaningful argument bytes
//!  6      command class
//!  7      command id      bit 7 set = read ("get") variant
//!  8..88  arguments[80]
//!  88     crc             xor of bytes 2..88
//!  89     reserved
//! ```
//! On Windows the HID feature report carries a leading report-id byte (0x00),
//! so the buffer handed to the OS is 91 bytes.

pub const REPORT_LEN: usize = 90;
pub const HID_BUF_LEN: usize = REPORT_LEN + 1;

/// Transaction id used to talk to the mouse itself (relayed by the HyperSpeed dongle).
pub const TID_MOUSE: u8 = 0x1f;
/// Transaction id used to talk to the dongle (its own firmware, LED indicator, ...).
pub const TID_DONGLE: u8 = 0xff;

pub const VARSTORE: u8 = 0x01;
pub const NOSTORE: u8 = 0x00;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    New = 0x00,
    Busy = 0x01,
    Successful = 0x02,
    Failure = 0x03,
    Timeout = 0x04,
    NotSupported = 0x05,
}

impl Status {
    pub fn from_byte(b: u8) -> Option<Status> {
        Some(match b {
            0x00 => Status::New,
            0x01 => Status::Busy,
            0x02 => Status::Successful,
            0x03 => Status::Failure,
            0x04 => Status::Timeout,
            0x05 => Status::NotSupported,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub status: u8,
    pub transaction_id: u8,
    pub remaining_packets: u16,
    pub protocol_type: u8,
    pub data_size: u8,
    pub command_class: u8,
    pub command_id: u8,
    pub arguments: [u8; 80],
}

impl Report {
    pub fn new(transaction_id: u8, command_class: u8, command_id: u8, data_size: u8) -> Report {
        Report {
            status: 0,
            transaction_id,
            remaining_packets: 0,
            protocol_type: 0,
            data_size,
            command_class,
            command_id,
            arguments: [0; 80],
        }
    }

    pub fn with_args(mut self, args: &[u8]) -> Report {
        self.arguments[..args.len()].copy_from_slice(args);
        self
    }

    pub fn status(&self) -> Option<Status> {
        Status::from_byte(self.status)
    }

    /// Serialize into the 91-byte Windows HID feature buffer (report id 0 + 90 bytes).
    pub fn to_hid_buffer(&self) -> [u8; HID_BUF_LEN] {
        let mut b = [0u8; HID_BUF_LEN];
        let r = &mut b[1..];
        r[0] = self.status;
        r[1] = self.transaction_id;
        r[2..4].copy_from_slice(&self.remaining_packets.to_be_bytes());
        r[4] = self.protocol_type;
        r[5] = self.data_size;
        r[6] = self.command_class;
        r[7] = self.command_id;
        r[8..88].copy_from_slice(&self.arguments);
        r[88] = crc(r);
        r[89] = 0;
        b
    }

    pub fn from_hid_buffer(b: &[u8; HID_BUF_LEN]) -> Report {
        let r = &b[1..];
        let mut arguments = [0u8; 80];
        arguments.copy_from_slice(&r[8..88]);
        Report {
            status: r[0],
            transaction_id: r[1],
            remaining_packets: u16::from_be_bytes([r[2], r[3]]),
            protocol_type: r[4],
            data_size: r[5],
            command_class: r[6],
            command_id: r[7],
            arguments,
        }
    }

    /// True when the response was produced for this request (same class/id).
    pub fn matches(&self, request: &Report) -> bool {
        self.command_class == request.command_class && self.command_id == request.command_id
    }
}

/// XOR of bytes 2..88 of the 90-byte report.
pub fn crc(report: &[u8]) -> u8 {
    report[2..88].iter().fold(0u8, |acc, b| acc ^ b)
}

// ---------------------------------------------------------------------------
// Command builders. Class/id/size and argument layouts follow the OpenRazer
// driver and the razerqdhid protocol notes; see FEATURES.md for sources.
// ---------------------------------------------------------------------------

pub mod cmd {
    use super::{Report, TID_DONGLE, TID_MOUSE, VARSTORE};

    pub fn firmware_version(tid: u8) -> Report { Report::new(tid, 0x00, 0x81, 0x02) }
    pub fn serial(tid: u8) -> Report { Report::new(tid, 0x00, 0x82, 0x16) }
    pub fn get_device_mode() -> Report { Report::new(TID_MOUSE, 0x00, 0x84, 0x02) }
    /// mode 0x00 = normal, 0x03 = driver mode.
    pub fn set_device_mode(mode: u8) -> Report {
        Report::new(TID_MOUSE, 0x00, 0x04, 0x02).with_args(&[mode, 0x00])
    }

    pub fn get_battery_level() -> Report { Report::new(TID_MOUSE, 0x07, 0x80, 0x02) }
    pub fn get_charging_status() -> Report { Report::new(TID_MOUSE, 0x07, 0x84, 0x02) }

    /// Idle timeout before the mouse sleeps, in seconds (60..=900).
    pub fn set_idle_time(seconds: u16) -> Report {
        let s = seconds.clamp(60, 900);
        Report::new(TID_MOUSE, 0x07, 0x03, 0x02).with_args(&s.to_be_bytes())
    }
    pub fn get_idle_time() -> Report { Report::new(TID_MOUSE, 0x07, 0x83, 0x02) }

    /// Low power mode threshold, percent (5..=100, encoded on 0..=255). The firmware only acts on it at polling rates of 1000 Hz and below.
    pub fn set_low_battery_threshold(percent: u8) -> Report {
        Report::new(TID_MOUSE, 0x07, 0x01, 0x01).with_args(&[percent_to_byte(percent)])
    }
    pub fn get_low_battery_threshold() -> Report { Report::new(TID_MOUSE, 0x07, 0x81, 0x01) }

    /// Current DPI (X, Y). Range 100..=35000 on the Viper V3 Pro (Focus Pro 35K).
    pub fn set_dpi_xy(dpi_x: u16, dpi_y: u16) -> Report {
        let x = dpi_x.to_be_bytes();
        let y = dpi_y.to_be_bytes();
        Report::new(TID_MOUSE, 0x04, 0x05, 0x07).with_args(&[VARSTORE, x[0], x[1], y[0], y[1], 0, 0])
    }
    pub fn get_dpi_xy() -> Report {
        Report::new(TID_MOUSE, 0x04, 0x85, 0x07).with_args(&[VARSTORE])
    }

    /// Up to 5 DPI stages, `active` is 1-based.
    pub fn set_dpi_stages(active: u8, stages: &[(u16, u16)]) -> Report {
        let count = stages.len().min(5) as u8;
        let mut args = vec![VARSTORE, active, count];
        for (i, (x, y)) in stages.iter().take(5).enumerate() {
            args.push(i as u8);
            args.extend_from_slice(&x.to_be_bytes());
            args.extend_from_slice(&y.to_be_bytes());
            args.extend_from_slice(&[0, 0]);
        }
        Report::new(TID_MOUSE, 0x04, 0x06, 0x26).with_args(&args)
    }
    pub fn get_dpi_stages() -> Report {
        Report::new(TID_MOUSE, 0x04, 0x86, 0x26).with_args(&[VARSTORE])
    }

    /// Extended polling rate command used by HyperPolling-capable mice.
    /// Razer sends it twice, once with `argument` 0x00 and once with 0x01.
    pub fn set_polling_rate(hz: u16, argument: u8) -> Report {
        Report::new(TID_MOUSE, 0x00, 0x40, 0x02).with_args(&[argument, polling_code(hz)])
    }
    pub fn get_polling_rate() -> Report { Report::new(TID_MOUSE, 0x00, 0xC0, 0x01) }

    /// HyperPolling dongle indicator LED mode (1..=3).
    pub fn set_dongle_led_mode(mode: u8) -> Report {
        Report::new(TID_DONGLE, 0x07, 0x10, 0x01).with_args(&[mode.clamp(1, 3)])
    }
    pub fn get_dongle_led_mode() -> Report { Report::new(TID_DONGLE, 0x07, 0x90, 0x01) }

    // --- Button mapping (class 0x02), onboard profiles ---

    /// `profile` 0 = direct/volatile, 1..=5 onboard; `hypershift` layer flag.
    pub fn set_button_function(profile: u8, button: u8, hypershift: bool, function: &[u8; 7]) -> Report {
        let mut a = vec![profile, button, hypershift as u8];
        a.extend_from_slice(function);
        Report::new(TID_MOUSE, 0x02, 0x0c, 0x0a).with_args(&a)
    }
    pub fn get_button_function(profile: u8, button: u8, hypershift: bool) -> Report {
        Report::new(TID_MOUSE, 0x02, 0x8c, 0x0a).with_args(&[profile, button, hypershift as u8])
    }

    // --- Onboard profiles (class 0x05) ---

    /// How many onboard profiles exist right now.
    pub fn get_profile_count() -> Report { Report::new(TID_MOUSE, 0x05, 0x80, 0x01) }
    /// The most profiles the mouse can hold (the Viper V3 Pro answers 1).
    pub fn get_max_profiles() -> Report { Report::new(TID_MOUSE, 0x05, 0x8a, 0x01) }
    /// Ids of the existing profiles: [count, id, id, ...].
    pub fn get_profile_list() -> Report { Report::new(TID_MOUSE, 0x05, 0x81, 0x50) }
    /// Active profile id (the Viper V3 Pro answers Failure: it has a single profile).
    pub fn get_active_profile() -> Report { Report::new(TID_MOUSE, 0x05, 0x84, 0x01) }
    pub fn set_active_profile(id: u8) -> Report { Report::new(TID_MOUSE, 0x05, 0x04, 0x01).with_args(&[id]) }
    /// Create / delete an onboard profile slot.
    pub fn create_profile(id: u8) -> Report { Report::new(TID_MOUSE, 0x05, 0x02, 0x01).with_args(&[id]) }
    pub fn delete_profile(id: u8) -> Report { Report::new(TID_MOUSE, 0x05, 0x03, 0x01).with_args(&[id]) }

    // --- Dongle pairing (class 0x00), in the order Synapse's pairing flow sends them ---

    /// Pairing mode 1 = start looking for a device with this product id, 4 = cancel.
    pub const PAIRING_START: u8 = 1;
    pub const PAIRING_CANCEL: u8 = 4;
    pub fn set_pairing_mode(tid: u8, mode: u8, product_id: u16) -> Report {
        let p = product_id.to_be_bytes();
        Report::new(tid, 0x00, 0x41, 0x03).with_args(&[mode, p[0], p[1]])
    }
    /// Forget the pairing with the device that has this product id.
    pub fn unpair(tid: u8, product_id: u16) -> Report {
        Report::new(tid, 0x00, 0x42, 0x02).with_args(&product_id.to_be_bytes())
    }
    /// Device scan control (2 pair, 3 searching, 4 cancel, 6 initial scan, 9 active search, 11 active pair).
    pub fn device_scan(tid: u8, mode: u8) -> Report {
        Report::new(tid, 0x00, 0x46, 0x01).with_args(&[mode])
    }
    pub fn get_auto_pairing(tid: u8) -> Report { Report::new(tid, 0x00, 0xd5, 0x01) }
    pub fn set_auto_pairing(tid: u8, on: bool) -> Report { Report::new(tid, 0x00, 0x55, 0x01).with_args(&[on as u8]) }
    /// Paired devices and their link state: [count, (status, pid hi, pid lo)...].
    pub fn get_paired_devices(tid: u8) -> Report { Report::new(tid, 0x00, 0xbf, 0x50) }
    /// Wireless link state of the device (1 byte).
    pub fn get_wireless_connection_status(tid: u8) -> Report { Report::new(tid, 0x00, 0xb8, 0x01) }

    pub fn polling_code(hz: u16) -> u8 {
        match hz {
            8000 => 0x01,
            4000 => 0x02,
            2000 => 0x04,
            1000 => 0x08,
            500 => 0x10,
            250 => 0x20,
            125 => 0x40,
            _ => 0x08,
        }
    }

    pub fn polling_from_code(code: u8) -> Option<u16> {
        Some(match code {
            0x01 => 8000,
            0x02 => 4000,
            0x04 => 2000,
            0x08 => 1000,
            0x10 => 500,
            0x20 => 250,
            0x40 => 125,
            _ => return None,
        })
    }

    /// Battery/threshold values travel as a byte on a 0..=255 scale.
    pub fn percent_to_byte(p: u8) -> u8 {
        ((p.min(100) as u32 * 255 + 50) / 100) as u8
    }
    pub fn byte_to_percent(b: u8) -> u8 {
        ((b as u32 * 100 + 127) / 255) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_roundtrip_and_crc() {
        let r = cmd::set_dpi_xy(1600, 800);
        let b = r.to_hid_buffer();
        assert_eq!(b[0], 0);
        assert_eq!(b[1], 0);
        assert_eq!(b[2], TID_MOUSE);
        assert_eq!(b[6], 0x07);
        assert_eq!(b[7], 0x04);
        assert_eq!(b[8], 0x05);
        assert_eq!(&b[9..16], &[0x01, 0x06, 0x40, 0x03, 0x20, 0, 0]);
        assert_eq!(b[89], crc(&b[1..]));
        let back = Report::from_hid_buffer(&b);
        assert_eq!(back, r);
    }

    #[test]
    fn dpi_stages_layout() {
        let r = cmd::set_dpi_stages(2, &[(400, 400), (800, 800), (1600, 1600)]);
        let a = &r.arguments;
        assert_eq!(&a[..3], &[VARSTORE, 2, 3]);
        assert_eq!(&a[3..10], &[0, 0x01, 0x90, 0x01, 0x90, 0, 0]);
        assert_eq!(&a[10..17], &[1, 0x03, 0x20, 0x03, 0x20, 0, 0]);
        assert_eq!(&a[17..24], &[2, 0x06, 0x40, 0x06, 0x40, 0, 0]);
        assert_eq!(r.data_size, 0x26);
    }

    #[test]
    fn polling_codes() {
        for hz in [125u16, 250, 500, 1000, 2000, 4000, 8000] {
            assert_eq!(cmd::polling_from_code(cmd::polling_code(hz)), Some(hz));
        }
        assert_eq!(cmd::polling_from_code(0x33), None);
    }

    #[test]
    fn percent_scaling() {
        assert_eq!(cmd::percent_to_byte(100), 255);
        assert_eq!(cmd::percent_to_byte(0), 0);
        assert_eq!(cmd::byte_to_percent(255), 100);
        assert_eq!(cmd::byte_to_percent(0x3f), 25);
        assert_eq!(cmd::byte_to_percent(0x0c), 5);
    }

}
