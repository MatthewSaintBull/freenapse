//! Dynamic Sensitivity (Synapse's name for onboard acceleration): the sensor
//! multiplies its output by a factor that depends on the movement speed.
//!
//! The mouse stores a state (off/on), a mode and, for the custom mode, a curve
//! of 250 `f32` multipliers indexed by speed. The three preset modes are built
//! into the firmware. Synapse's editor describes a custom curve with a handful
//! of control points, draws a monotone cubic spline through them and samples
//! it at speeds 1..=250; the port below reproduces that exactly, including the
//! spline (Fritsch-Carlson slopes with Synapse's end-point rule).

use crate::protocol::{Report, TID_MOUSE};

/// Number of speed samples the firmware stores for a custom curve.
pub const POINTS: usize = 250;
/// Samples per packet when writing or reading the curve.
pub const CHUNK: usize = 8;
/// Lowest multiplier Synapse allows.
pub const MIN_MULTIPLIER: f32 = 0.1;
/// Highest multiplier of Synapse's custom editor grid.
pub const MAX_MULTIPLIER: f32 = 1.5;
/// Highest speed of Synapse's custom editor grid (7 steps of 15).
pub const MAX_SPEED: u8 = 105;
/// Most control points the editor keeps.
pub const MAX_CONTROL_POINTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Classic = 0,
    Natural = 1,
    Jump = 2,
    Custom = 3,
}

impl Mode {
    pub fn from_byte(b: u8) -> Option<Mode> {
        Some(match b {
            0 => Mode::Classic,
            1 => Mode::Natural,
            2 => Mode::Jump,
            3 => Mode::Custom,
            _ => return None,
        })
    }

    pub fn from_name(s: &str) -> Option<Mode> {
        Some(match s {
            "classic" => Mode::Classic,
            "natural" => Mode::Natural,
            "jump" => Mode::Jump,
            "custom" => Mode::Custom,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Classic => "classic",
            Mode::Natural => "natural",
            Mode::Jump => "jump",
            Mode::Custom => "custom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Classic => "Classic",
            Mode::Natural => "Natural",
            Mode::Jump => "Jump",
            Mode::Custom => "Custom",
        }
    }

    /// Synapse's control points for the preset curves (also the templates a
    /// custom curve starts from). `(speed, multiplier)`.
    pub fn template(self) -> Vec<(u8, f32)> {
        match self {
            Mode::Classic => vec![(0, 1.0), (20, 1.1), (40, 1.2), (70, 1.35), (100, 1.5)],
            Mode::Natural => vec![(0, 1.0), (11, 1.45), (20, 1.5), (35, 1.5), (100, 1.5)],
            Mode::Jump => vec![(0, 1.0), (9, 1.0), (21, 1.5), (24, 1.5), (100, 1.5)],
            Mode::Custom => Mode::Classic.template(),
        }
    }
}

/// Check and tidy a list of control points: first at speed 0, speeds strictly
/// increasing and at most [`MAX_SPEED`], multipliers inside the editor's range.
pub fn validate_points(points: &[(u8, f32)]) -> Result<(), String> {
    if points.len() < 2 {
        return Err("the curve needs at least 2 control points".into());
    }
    if points.len() > MAX_CONTROL_POINTS {
        return Err(format!("the curve can have at most {MAX_CONTROL_POINTS} control points"));
    }
    if points[0].0 != 0 {
        return Err("the first control point must be at speed 0".into());
    }
    for w in points.windows(2) {
        if w[1].0 <= w[0].0 {
            return Err("control point speeds must increase".into());
        }
    }
    for (x, y) in points {
        if *x > MAX_SPEED {
            return Err(format!("control point speed must be 0..{MAX_SPEED}"));
        }
        if !(MIN_MULTIPLIER..=MAX_MULTIPLIER).contains(y) || y.is_nan() {
            return Err(format!("multiplier must be {MIN_MULTIPLIER}..{MAX_MULTIPLIER}"));
        }
    }
    Ok(())
}

/// Monotone cubic spline through `(x, y)` points, as Synapse's editor builds it.
pub struct Spline {
    x: Vec<f64>,
    c0: Vec<f64>,
    c1: Vec<f64>,
    c2: Vec<f64>,
    c3: Vec<f64>,
}

impl Spline {
    pub fn new(xs: &[f64], ys: &[f64]) -> Spline {
        let n = xs.len();
        assert!(n >= 2 && n == ys.len());
        // Secant slopes.
        let d: Vec<f64> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i])).collect();
        let mut t = vec![0.0; n];
        if n == 2 {
            t[0] = d[0];
            t[1] = d[0];
        } else {
            let mut h_prev = xs[1] - xs[0];
            let mut prev_zero = d[0].abs() < f64::EPSILON;
            for i in 1..n - 1 {
                let h_next = xs[i + 1] - xs[i];
                let zero = d[i].abs() < f64::EPSILON;
                if zero || prev_zero || d[i].signum() != d[i - 1].signum() {
                    t[i] = 0.0;
                } else {
                    let w1 = 2.0 * h_next + h_prev;
                    let w2 = h_next + 2.0 * h_prev;
                    t[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
                }
                h_prev = h_next;
                prev_zero = zero;
            }
            t[0] = end_slope(xs[1] - xs[0], xs[2] - xs[1], d[0], d[1]);
            t[n - 1] = end_slope(xs[n - 1] - xs[n - 2], xs[n - 2] - xs[n - 3], d[n - 2], d[n - 3]);
        }
        let mut c0 = Vec::with_capacity(n - 1);
        let mut c1 = Vec::with_capacity(n - 1);
        let mut c2 = Vec::with_capacity(n - 1);
        let mut c3 = Vec::with_capacity(n - 1);
        for i in 0..n - 1 {
            let h = xs[i + 1] - xs[i];
            c0.push(ys[i]);
            c1.push(t[i]);
            c2.push((3.0 * (ys[i + 1] - ys[i]) / h - 2.0 * t[i] - t[i + 1]) / h);
            c3.push((2.0 * (ys[i] - ys[i + 1]) / h + t[i] + t[i + 1]) / (h * h));
        }
        Spline { x: xs.to_vec(), c0, c1, c2, c3 }
    }

    pub fn interpolate(&self, x: f64) -> f64 {
        // Segment index: last knot <= x, clamped to the segments.
        let mut i = 0;
        while i + 1 < self.c0.len() && x >= self.x[i + 1] {
            i += 1;
        }
        let dx = x - self.x[i];
        self.c0[i] + dx * (self.c1[i] + dx * (self.c2[i] + dx * self.c3[i]))
    }
}

/// Synapse's one-sided end-point slope.
fn end_slope(h0: f64, h1: f64, d0: f64, d1: f64) -> f64 {
    let s = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
    if s.signum() != d0.signum() {
        0.0
    } else if d0.signum() != d1.signum() && s.abs() > 3.0 * d0.abs() {
        3.0 * d0
    } else {
        s
    }
}

/// The 250 multipliers the mouse stores for a set of control points, computed
/// the way Synapse's editor does: 1000 spline samples over `0..=last speed`,
/// then one value per integer speed 1..=250 (flat beyond the last point).
pub fn curve(points: &[(u8, f32)]) -> [f32; POINTS] {
    let xs: Vec<f64> = points.iter().map(|p| p.0 as f64).collect();
    let ys: Vec<f64> = points.iter().map(|p| p.1 as f64).collect();
    let spline = Spline::new(&xs, &ys);
    let last_x = *xs.last().unwrap_or(&0.0);
    let samples: Vec<(f64, f64)> = (0..1000)
        .map(|i| {
            let x = i as f64 * last_x / 999.0;
            (x, spline.interpolate(x))
        })
        .collect();
    let mut out = [0f32; POINTS];
    let mut last = 0f64;
    for (i, slot) in out.iter_mut().enumerate() {
        let speed = (i + 1) as f64;
        let mut found = None;
        for (x, y) in &samples {
            if x.round() == speed {
                found = Some(*y);
                break;
            }
            if *x > speed {
                break;
            }
        }
        if let Some(y) = found {
            last = y;
        }
        *slot = last.max(MIN_MULTIPLIER as f64) as f32;
    }
    out
}

/// Bytes of one curve packet: 8 little-endian `f32` values.
pub fn chunk_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

// --- command builders (class 0x0b); `profile` is the onboard profile id ---

pub fn set_state(profile: u8, on: bool) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x10, 0x02).with_args(&[profile, on as u8])
}
pub fn get_state(profile: u8) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x90, 0x02).with_args(&[profile])
}
pub fn set_mode(profile: u8, mode: Mode) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x11, 0x02).with_args(&[profile, mode as u8])
}
pub fn get_mode(profile: u8) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x91, 0x02).with_args(&[profile])
}
/// One packet of the custom curve: `offset` and `count` in samples (count <= 8).
pub fn set_curve_chunk(profile: u8, mode: Mode, offset: u8, values: &[f32]) -> Report {
    let mut a = vec![profile, mode as u8, POINTS as u8, offset, values.len().min(CHUNK) as u8];
    a.extend(chunk_bytes(&values[..values.len().min(CHUNK)]));
    Report::new(TID_MOUSE, 0x0b, 0x13, 0x25).with_args(&a)
}
pub fn get_curve_chunk(profile: u8, mode: Mode, offset: u8, count: u8) -> Report {
    Report::new(TID_MOUSE, 0x0b, 0x93, 0x25).with_args(&[profile, mode as u8, POINTS as u8, offset, count.min(CHUNK as u8)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_valid() {
        for m in [Mode::Classic, Mode::Natural, Mode::Jump] {
            validate_points(&m.template()).unwrap();
        }
        assert!(validate_points(&[(0, 1.0)]).is_err());
        assert!(validate_points(&[(1, 1.0), (10, 1.2)]).is_err());
        assert!(validate_points(&[(0, 1.0), (10, 1.2), (10, 1.3)]).is_err());
        assert!(validate_points(&[(0, 1.0), (10, 2.0)]).is_err());
    }

    #[test]
    fn spline_passes_through_points_and_is_monotone() {
        let pts = Mode::Classic.template();
        let xs: Vec<f64> = pts.iter().map(|p| p.0 as f64).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1 as f64).collect();
        let s = Spline::new(&xs, &ys);
        for (x, y) in xs.iter().zip(&ys) {
            assert!((s.interpolate(*x) - y).abs() < 1e-9, "at {x}");
        }
        let mut prev = s.interpolate(0.0);
        for i in 1..=1000 {
            let v = s.interpolate(i as f64 / 10.0);
            assert!(v >= prev - 1e-9);
            prev = v;
        }
    }

    #[test]
    fn curve_shape() {
        let c = curve(&Mode::Classic.template());
        assert!((c[0] - 1.0).abs() < 0.02);
        assert!((c[99] - 1.5).abs() < 0.01);
        assert_eq!(c[249], c[99]);
        let j = curve(&Mode::Jump.template());
        assert!((j[5] - 1.0).abs() < 1e-6);
        assert!((j[30] - 1.5).abs() < 1e-6);
        // Flat segment: a constant run between two equal points stays constant.
        let flat = curve(&[(0, 1.0), (50, 1.0), (100, 1.0)]);
        assert!(flat.iter().all(|v| (*v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn packet_layout() {
        let r = set_curve_chunk(1, Mode::Custom, 16, &[1.0; 8]);
        assert_eq!(r.data_size, 0x25);
        assert_eq!(&r.arguments[..5], &[1, 3, 250, 16, 8]);
        assert_eq!(&r.arguments[5..9], &1.0f32.to_le_bytes());
        assert_eq!(get_curve_chunk(1, Mode::Custom, 248, 2).arguments[..5], [1, 3, 250, 248, 2]);
        assert_eq!(set_state(1, true).arguments[..2], [1, 1]);
        assert_eq!(set_mode(2, Mode::Jump).arguments[..2], [2, 2]);
    }
}
