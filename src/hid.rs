//! Minimal Windows HID transport: SetupAPI enumeration + HidD feature reports.
//! No C code, no hidapi; only kernel32/setupapi/hid import libraries.

#![allow(non_snake_case, unsafe_op_in_unsafe_fn)]

use std::{fmt, mem, ptr, thread, time::Duration};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
use windows_sys::Win32::Devices::HumanInterfaceDevice::*;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::core::GUID;

use crate::protocol::{HID_BUF_LEN, Report, Status};

#[derive(Debug, Clone)]
pub enum HidError {
    /// No matching HID control interface is present.
    NotFound,
    /// A Win32 call failed (name, GetLastError).
    Win32(&'static str, u32),
    /// The device answered with a non-success status.
    Status(Status),
    /// The device answered with a status byte outside the known set.
    BadStatus(u8),
    /// No response for our request arrived (another client may have overwritten it).
    NoResponse,
    /// The command only makes sense on the wireless dongle / only wired.
    Unsupported(&'static str),
}

impl fmt::Display for HidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HidError::NotFound => write!(f, "Razer mouse not found (is the dongle plugged in?)"),
            HidError::Win32(call, code) => write!(f, "{call} failed (Win32 error {code})"),
            HidError::Status(Status::Timeout) => {
                write!(f, "mouse did not answer (asleep, switched off, or out of range?)")
            }
            HidError::Status(s) => write!(f, "device returned status {s:?}"),
            HidError::BadStatus(b) => write!(f, "device returned unknown status 0x{b:02x}"),
            HidError::NoResponse => write!(f, "no response matched the request"),
            HidError::Unsupported(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for HidError {}

pub type Result<T> = std::result::Result<T, HidError>;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub feature_report_len: u16,
}

pub struct HidDevice {
    handle: HANDLE,
    pub info: DeviceInfo,
    /// Delay between sending a request and reading its response.
    pub wait: Duration,
}

unsafe impl Send for HidDevice {}

impl Drop for HidDevice {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// All HID interface paths whose device path contains `needle` (case-insensitive).
pub fn interface_paths(needle: &str) -> Vec<String> {
    let needle = needle.to_lowercase();
    let mut out = Vec::new();
    unsafe {
        let mut guid: GUID = mem::zeroed();
        HidD_GetHidGuid(&mut guid);
        let set = SetupDiGetClassDevsW(&guid, ptr::null(), 0, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        if set == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut idx = 0;
        loop {
            let mut iface: SP_DEVICE_INTERFACE_DATA = mem::zeroed();
            iface.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            if SetupDiEnumDeviceInterfaces(set, ptr::null(), &guid, idx, &mut iface) == 0 {
                break;
            }
            idx += 1;
            let mut required = 0u32;
            SetupDiGetDeviceInterfaceDetailW(set, &iface, ptr::null_mut(), 0, &mut required, ptr::null_mut());
            if required == 0 {
                continue;
            }
            let mut buf = vec![0u8; required as usize];
            let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(set, &iface, detail, required, ptr::null_mut(), ptr::null_mut()) == 0 {
                continue;
            }
            let p = ptr::addr_of!((*detail).DevicePath) as *const u16;
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            let path = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            if path.to_lowercase().contains(&needle) {
                out.push(path);
            }
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    out
}

impl HidDevice {
    /// Open a HID interface path. Falls back to a zero-access handle when the
    /// collection is owned by the mouse/keyboard class driver (feature reports still work).
    pub fn open_path(path: &str) -> Result<HidDevice> {
        unsafe {
            let w = wide(path);
            let share = FILE_SHARE_READ | FILE_SHARE_WRITE;
            let mut h = CreateFileW(w.as_ptr(), GENERIC_READ | GENERIC_WRITE, share, ptr::null(), OPEN_EXISTING, 0, 0);
            if h == INVALID_HANDLE_VALUE {
                h = CreateFileW(w.as_ptr(), 0, share, ptr::null(), OPEN_EXISTING, 0, 0);
            }
            if h == INVALID_HANDLE_VALUE {
                return Err(HidError::Win32("CreateFileW", GetLastError()));
            }
            let mut attrs: HIDD_ATTRIBUTES = mem::zeroed();
            attrs.Size = mem::size_of::<HIDD_ATTRIBUTES>() as u32;
            if HidD_GetAttributes(h, &mut attrs) == 0 {
                let e = GetLastError();
                CloseHandle(h);
                return Err(HidError::Win32("HidD_GetAttributes", e));
            }
            let mut caps: HIDP_CAPS = mem::zeroed();
            let mut pp: PHIDP_PREPARSED_DATA = 0;
            if HidD_GetPreparsedData(h, &mut pp) != 0 {
                HidP_GetCaps(pp, &mut caps);
                HidD_FreePreparsedData(pp);
            }
            Ok(HidDevice {
                handle: h,
                info: DeviceInfo {
                    path: path.to_string(),
                    vendor_id: attrs.VendorID,
                    product_id: attrs.ProductID,
                    usage_page: caps.UsagePage,
                    usage: caps.Usage,
                    feature_report_len: caps.FeatureReportByteLength,
                },
                wait: Duration::from_millis(150),
            })
        }
    }

    /// Find the Razer control collection (the one exposing a 91-byte feature report)
    /// for any of the given product ids.
    pub fn open_control(vendor_id: u16, product_ids: &[u16]) -> Result<HidDevice> {
        let needle = format!("vid_{vendor_id:04x}");
        for path in interface_paths(&needle) {
            let Ok(dev) = HidDevice::open_path(&path) else { continue };
            if product_ids.contains(&dev.info.product_id) && dev.info.feature_report_len as usize >= HID_BUF_LEN {
                return Ok(dev);
            }
        }
        Err(HidError::NotFound)
    }

    pub fn set_feature(&self, buf: &[u8; HID_BUF_LEN]) -> Result<()> {
        let ok = unsafe { HidD_SetFeature(self.handle, buf.as_ptr() as *const _, HID_BUF_LEN as u32) };
        if ok == 0 {
            return Err(HidError::Win32("HidD_SetFeature", unsafe { GetLastError() }));
        }
        Ok(())
    }

    pub fn get_feature(&self) -> Result<[u8; HID_BUF_LEN]> {
        let mut buf = [0u8; HID_BUF_LEN];
        let ok = unsafe { HidD_GetFeature(self.handle, buf.as_mut_ptr() as *mut _, HID_BUF_LEN as u32) };
        if ok == 0 {
            return Err(HidError::Win32("HidD_GetFeature", unsafe { GetLastError() }));
        }
        Ok(buf)
    }

    /// Send a request and wait for its response.
    ///
    /// The dongle keeps a single response slot. Reading it too early yields a
    /// `Timeout` status and aborts the exchange, and another client (Synapse)
    /// may overwrite it with its own traffic, so the exchange is retried a few times.
    pub fn transact(&self, request: &Report) -> Result<Report> {
        // The dongle has one response slot: one exchange at a time in this process
        // (the tray app and the settings window's I/O thread both use it).
        static BUS: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _bus = BUS.lock().unwrap_or_else(|e| e.into_inner());
        let t0 = std::time::Instant::now();
        let r = self.transact_inner(request);
        if let Some(p) = std::env::var_os("FREENAPSE_HIDLOG") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(p) {
                let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 100_000).unwrap_or(0);
                let _ = writeln!(f, "{:>5} {:?} tid {:02x} {:02x}/{:02x} {} ms {}", ms, std::thread::current().id(), request.transaction_id, request.command_class, request.command_id, t0.elapsed().as_millis(), if r.is_ok() { "ok" } else { "ERR" });
            }
        }
        r
    }

    fn transact_inner(&self, request: &Report) -> Result<Report> {
        let mut last = HidError::NoResponse;
        for attempt in 0..4 {
            if attempt > 0 {
                thread::sleep(Duration::from_millis(80 * attempt as u64));
            }
            self.set_feature(&request.to_hid_buffer())?;
            thread::sleep(self.wait);
            for _ in 0..5 {
                let resp = Report::from_hid_buffer(&self.get_feature()?);
                if !resp.matches(request) || resp.transaction_id != request.transaction_id {
                    // Someone else's exchange; give ours a moment, then resend.
                    last = HidError::NoResponse;
                    thread::sleep(Duration::from_millis(30));
                    continue;
                }
                match resp.status() {
                    Some(Status::Successful) => return Ok(resp),
                    Some(Status::Busy) | Some(Status::New) => {
                        thread::sleep(Duration::from_millis(30));
                        continue;
                    }
                    Some(Status::Timeout) => {
                        last = HidError::Status(Status::Timeout);
                        break;
                    }
                    Some(s) => return Err(HidError::Status(s)),
                    None => return Err(HidError::BadStatus(resp.status)),
                }
            }
        }
        Err(last)
    }
}
