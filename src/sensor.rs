//! Focus Pro sensor tracking settings, written the way Synapse 4 writes them
//! for the Viper V3 Pro.
//!
//! Synapse offers two paths for this mouse and nothing else, no surface calibration:
//!
//! * **Smart Tracking** (`LiftMode` 1 "Universal lift cut-off"): calibration
//!   state off, lift height code 0..=2 for 1, 2 or 3 mm.
//! * **Asymmetric cut-off** (`LiftMode` 4 "Preset calibration hysteresis"):
//!   calibration state on, lift mode 4, then a two-byte configuration
//!   `[lift-off - 1, landing - 1]` with lift-off 2..=26 and landing 1..=25
//!   (landing below lift-off). Synapse's defaults are 13 / 12.

use crate::protocol::{Report, TID_MOUSE};

/// First two argument bytes of every sensor command: class id 0, PixArt sensor id 4.
pub const SENSOR_HDR: [u8; 2] = [0x00, 0x04];

pub const LIFT_MODE_SMART: u8 = 1;
pub const LIFT_MODE_ASYMMETRIC: u8 = 4;

pub const ASYM_LIFT_MIN: u8 = 2;
pub const ASYM_LIFT_MAX: u8 = 26;
pub const ASYM_LAND_MIN: u8 = 1;
pub const ASYM_LAND_MAX: u8 = 25;
/// Synapse's default asymmetric values.
pub const ASYM_LIFT_DEFAULT: u8 = 13;
pub const ASYM_LAND_DEFAULT: u8 = 12;

/// What to write to the sensor for one tracking configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorPlan {
    /// 0x0b/0x03: 0 = smart tracking, 1 = calibration data in use.
    pub state: bool,
    /// 0x0b/0x0b: (liftMode, liftHeight).
    pub lift: (u8, u8),
    /// 0x0b/0x05 configuration bytes (asymmetric: lift-off - 1, landing - 1).
    pub config: Option<[u8; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tracking {
    /// Smart tracking: symmetric distance 1..=3 mm.
    Smart { mm: u8 },
    /// Asymmetric cut-off: lift-off 2..=26, landing 1..=25, landing < lift-off.
    Asymmetric { lift: u8, land: u8 },
}

impl Tracking {
    pub fn plan(self) -> Result<SensorPlan, String> {
        Ok(match self {
            Tracking::Smart { mm } => {
                if !(1..=3).contains(&mm) {
                    return Err("smart tracking distance must be 1, 2 or 3 mm".into());
                }
                SensorPlan { state: false, lift: (LIFT_MODE_SMART, mm - 1), config: None }
            }
            Tracking::Asymmetric { lift, land } => {
                if !(ASYM_LIFT_MIN..=ASYM_LIFT_MAX).contains(&lift) {
                    return Err(format!("asymmetric lift-off must be {ASYM_LIFT_MIN}..{ASYM_LIFT_MAX}"));
                }
                if !(ASYM_LAND_MIN..=ASYM_LAND_MAX).contains(&land) {
                    return Err(format!("asymmetric landing must be {ASYM_LAND_MIN}..{ASYM_LAND_MAX}"));
                }
                if land >= lift {
                    return Err("asymmetric landing must be below the lift-off".into());
                }
                SensorPlan { state: true, lift: (LIFT_MODE_ASYMMETRIC, 0), config: Some([lift - 1, land - 1]) }
            }
        })
    }
}

// --- command builders (class 0x0b) ---

pub fn set_state(use_calibration: bool) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x03, 0x03).with_args(&[SENSOR_HDR[0], SENSOR_HDR[1], use_calibration as u8])
}
pub fn get_state() -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x83, 0x03).with_args(&SENSOR_HDR)
}
pub fn set_lift(mode: u8, height: u8) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x0b, 0x04).with_args(&[SENSOR_HDR[0], SENSOR_HDR[1], mode, height])
}
pub fn get_lift() -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x8b, 0x04).with_args(&SENSOR_HDR)
}
/// Sensor configuration (size 11: header + 9 bytes; the asymmetric mode uses two).
pub fn set_config(p: &[u8]) -> Report {
    let mut a = SENSOR_HDR.to_vec();
    a.extend_from_slice(&p[..p.len().min(9)]);
    Report::new(TID_MOUSE, 0x0b, 0x05, 0x0b).with_args(&a)
}
pub fn get_config() -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x85, 0x0b).with_args(&SENSOR_HDR)
}
/// Sensor rotation ("angle tune"): [1, enabled, angle as i8].
pub fn set_angle(enabled: bool, degrees: i8) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x14, 0x03).with_args(&[0x01, enabled as u8, degrees as u8])
}
pub fn get_angle() -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x94, 0x03).with_args(&[0x01])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans() {
        assert_eq!(Tracking::Smart { mm: 2 }.plan().unwrap(), SensorPlan { state: false, lift: (1, 1), config: None });
        assert!(Tracking::Smart { mm: 4 }.plan().is_err());
        let p = Tracking::Asymmetric { lift: 13, land: 12 }.plan().unwrap();
        assert_eq!(p, SensorPlan { state: true, lift: (4, 0), config: Some([12, 11]) });
        assert!(Tracking::Asymmetric { lift: 2, land: 2 }.plan().is_err());
        assert!(Tracking::Asymmetric { lift: 27, land: 1 }.plan().is_err());
        assert!(Tracking::Asymmetric { lift: 1, land: 1 }.plan().is_err());
    }

    #[test]
    fn command_bytes() {
        assert_eq!(set_config(&[12, 11]).arguments[..5], [0, 4, 12, 11, 0]);
        assert_eq!(set_config(&[12, 11]).data_size, 0x0b);
        assert_eq!(set_lift(4, 0).arguments[..4], [0, 4, 4, 0]);
        assert_eq!(set_angle(true, -15).arguments[..3], [1, 1, 0xf1]);
    }
}
