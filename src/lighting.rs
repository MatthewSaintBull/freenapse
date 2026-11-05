//! Lighting of the mice that have LEDs (the Basilisk V2's logo and scroll
//! wheel): one effect and one brightness per zone, stored in the mouse.
//!
//! Class 0x0f, the "extended" effect commands: set effect 0x0f/0x02
//! `[storage, led, effect, sub, speed, colours, r, g, b]`, get 0x0f/0x82;
//! brightness 0x0f/0x04 `[storage, led, 0..255]`, get 0x0f/0x84. Every
//! encoding below was written to a Basilisk V2 and read back.

use crate::protocol::{Report, TID_MOUSE};

pub const CLASS: u8 = 0x0f;
/// Persistent storage (the effect survives unplugging).
pub const VARSTORE: u8 = 0x01;

pub const EFFECT_OFF: u8 = 0x00;
pub const EFFECT_STATIC: u8 = 0x01;
pub const EFFECT_BREATHING: u8 = 0x02;
pub const EFFECT_SPECTRUM: u8 = 0x03;

/// One zone's effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Off,
    Static([u8; 3]),
    Breathing([u8; 3]),
    /// Breathing through random colours.
    BreathingRandom,
    /// Cycling through the colour wheel.
    Spectrum,
}

impl Effect {
    /// "off", "spectrum", "static:#ff0000", "breathing:#00ff00", "breathing:random".
    pub fn parse(s: &str) -> Result<Effect, String> {
        let s = s.trim();
        match s {
            "off" => return Ok(Effect::Off),
            "spectrum" => return Ok(Effect::Spectrum),
            "breathing:random" => return Ok(Effect::BreathingRandom),
            _ => {}
        }
        if let Some(c) = s.strip_prefix("static:") {
            return Ok(Effect::Static(parse_color(c)?));
        }
        if let Some(c) = s.strip_prefix("breathing:") {
            return Ok(Effect::Breathing(parse_color(c)?));
        }
        Err(format!("unknown lighting effect '{s}' (off, spectrum, static:#rrggbb, breathing:#rrggbb or breathing:random)"))
    }

    /// The config text, the inverse of [`Effect::parse`].
    pub fn spec(&self) -> String {
        match self {
            Effect::Off => "off".into(),
            Effect::Spectrum => "spectrum".into(),
            Effect::BreathingRandom => "breathing:random".into(),
            Effect::Static(c) => format!("static:{}", color_hex(*c)),
            Effect::Breathing(c) => format!("breathing:{}", color_hex(*c)),
        }
    }

    pub fn color(&self) -> Option<[u8; 3]> {
        match self {
            Effect::Static(c) | Effect::Breathing(c) => Some(*c),
            _ => None,
        }
    }
}

/// "#ff8000" or "ff8000".
pub fn parse_color(s: &str) -> Result<[u8; 3], String> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("bad colour '{s}': use #rrggbb"));
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
    Ok([b(0), b(2), b(4)])
}

pub fn color_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn set_effect(led: u8, e: Effect) -> Report {
    let args: Vec<u8> = match e {
        Effect::Off => vec![VARSTORE, led, EFFECT_OFF, 0, 0, 0],
        Effect::Spectrum => vec![VARSTORE, led, EFFECT_SPECTRUM, 0, 0, 0],
        Effect::BreathingRandom => vec![VARSTORE, led, EFFECT_BREATHING, 0, 0, 0],
        Effect::Static([r, g, b]) => vec![VARSTORE, led, EFFECT_STATIC, 0, 0, 1, r, g, b],
        Effect::Breathing([r, g, b]) => vec![VARSTORE, led, EFFECT_BREATHING, 1, 0, 1, r, g, b],
    };
    Report::new(TID_MOUSE, CLASS, 0x02, args.len() as u8).with_args(&args)
}

pub fn get_effect(led: u8) -> Report {
    Report::new(TID_MOUSE, CLASS, 0x82, 0x0c).with_args(&[VARSTORE, led])
}

/// The effect in a get-effect answer, when it is one freenapse can write.
pub fn decode_effect(a: &[u8]) -> Option<Effect> {
    let rgb = [a[6], a[7], a[8]];
    Some(match (a[2], a[3]) {
        (EFFECT_OFF, _) => Effect::Off,
        (EFFECT_SPECTRUM, _) => Effect::Spectrum,
        (EFFECT_STATIC, _) => Effect::Static(rgb),
        (EFFECT_BREATHING, 0) => Effect::BreathingRandom,
        (EFFECT_BREATHING, 1) => Effect::Breathing(rgb),
        _ => return None,
    })
}

pub fn set_brightness(led: u8, byte: u8) -> Report {
    Report::new(TID_MOUSE, CLASS, 0x04, 0x03).with_args(&[VARSTORE, led, byte])
}

pub fn get_brightness(led: u8) -> Report {
    Report::new(TID_MOUSE, CLASS, 0x84, 0x03).with_args(&[VARSTORE, led])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_round_trip() {
        for s in ["off", "spectrum", "breathing:random", "static:#ff0000", "breathing:#00ff7f"] {
            assert_eq!(Effect::parse(s).unwrap().spec(), s);
        }
        assert_eq!(Effect::parse("static:00FF00").unwrap(), Effect::Static([0, 255, 0]));
        assert!(Effect::parse("static:#12345").is_err());
        assert!(Effect::parse("wave").is_err());
    }

    #[test]
    fn encodings_match_the_basilisk_v2() {
        // The argument bytes as written to the mouse and read back.
        let r = set_effect(0x04, Effect::Static([0xff, 0, 0]));
        assert_eq!((r.command_class, r.command_id, r.data_size), (0x0f, 0x02, 9));
        assert_eq!(&r.arguments[..9], &[0x01, 0x04, 0x01, 0x00, 0x00, 0x01, 0xff, 0x00, 0x00]);
        let r = set_effect(0x04, Effect::Breathing([0, 0xff, 0]));
        assert_eq!(&r.arguments[..9], &[0x01, 0x04, 0x02, 0x01, 0x00, 0x01, 0x00, 0xff, 0x00]);
        let r = set_effect(0x01, Effect::Spectrum);
        assert_eq!((r.data_size, &r.arguments[..6]), (6, &[0x01, 0x01, 0x03, 0, 0, 0][..]));
        // Answers of the get command.
        let answer = |b: &[u8]| {
            let mut a = [0u8; 12];
            a[..b.len()].copy_from_slice(b);
            decode_effect(&a)
        };
        assert_eq!(answer(&[0x00, 0x04, 0x01, 0x00, 0x00, 0x01, 0xff, 0x00, 0x00]), Some(Effect::Static([0xff, 0, 0])));
        assert_eq!(answer(&[0x00, 0x04, 0x02, 0x01, 0x00, 0x01, 0x00, 0xff, 0x00]), Some(Effect::Breathing([0, 0xff, 0])));
        assert_eq!(answer(&[0x00, 0x04, 0x02, 0x00]), Some(Effect::BreathingRandom));
        assert_eq!(answer(&[0x00, 0x04, 0x03]), Some(Effect::Spectrum));
        assert_eq!(answer(&[0x00, 0x04, 0x00]), Some(Effect::Off));
        assert_eq!(answer(&[0x00, 0x04, 0x05]), None, "reactive is not written by freenapse");
    }
}
