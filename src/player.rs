//! Software macro playback: the recorded events are injected with `SendInput`.
//!
//! The Viper V3 Pro has no macro memory (every class 0x06 command answers
//! "not supported", and Synapse's device spec lists `macros: false` for it), so
//! Synapse plays macros from the PC. freenapse does the same: a button bound to
//! a macro gets one of the keys F13..F24 in the mouse, and the tray app's
//! keyboard hook swallows that key and plays the macro here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

use crate::macros::Event;

/// Marker in `dwExtraInfo` of the events we inject.
pub const INJECT_MARK: usize = 0x6672_6e70; // "frnp"

/// Virtual keys F13..F24 carry the software macro slots.
pub const VK_F13: u16 = 0x7c;
pub const VK_F24: u16 = 0x87;

/// Keyboard HID usage of a Windows virtual key (US layout for the punctuation keys).
pub fn vk_usage(vk: u16) -> Option<u8> {
    Some(match vk {
        0x41..=0x5a => 0x04 + (vk - 0x41) as u8,            // A..Z
        0x31..=0x39 => 0x1e + (vk - 0x31) as u8,            // 1..9
        0x30 => 0x27,                                       // 0
        0x0d => 0x28,                                       // Enter
        0x1b => 0x29,
        0x08 => 0x2a,                                       // Backspace
        0x09 => 0x2b,                                       // Tab
        0x20 => 0x2c,                                       // Space
        0xbd => 0x2d,                                       // -
        0xbb => 0x2e,                                       // =
        0xdb => 0x2f,                                       // [
        0xdd => 0x30,                                       // ]
        0xdc => 0x31,                                       // backslash
        0xba => 0x33,                                       // ;
        0xde => 0x34,                                       // '
        0xc0 => 0x35,                                       // `
        0xbc => 0x36,                                       // ,
        0xbe => 0x37,                                       // .
        0xbf => 0x38,                                       // /
        0x14 => 0x39,                                       // Caps lock
        0x70..=0x7b => 0x3a + (vk - 0x70) as u8,            // F1..F12
        0x7c..=0x87 => 0x68 + (vk - 0x7c) as u8,            // F13..F24
        0x2c => 0x46,                                       // Print screen
        0x91 => 0x47,                                       // Scroll lock
        0x13 => 0x48,                                       // Pause
        0x2d => 0x49,                                       // Insert
        0x24 => 0x4a,                                       // Home
        0x21 => 0x4b,                                       // Page up
        0x2e => 0x4c,                                       // Delete
        0x23 => 0x4d,                                       // End
        0x22 => 0x4e,                                       // Page down
        0x27 => 0x4f,                                       // Right
        0x25 => 0x50,                                       // Left
        0x28 => 0x51,                                       // Down
        0x26 => 0x52,                                       // Up
        0x90 => 0x53,                                       // Num lock
        0x6f => 0x54,                                       // keypad /
        0x6a => 0x55,                                       // keypad *
        0x6d => 0x56,                                       // keypad -
        0x6b => 0x57,                                       // keypad +
        0x61..=0x69 => 0x59 + (vk - 0x61) as u8,            // keypad 1..9
        0x60 => 0x62,                                       // keypad 0
        0x6e => 0x63,                                       // keypad .
        0xa2 => 0xe0,                                       // left ctrl
        0xa0 => 0xe1,                                       // left shift
        0xa4 => 0xe2,                                       // left alt
        0x5b => 0xe3,                                       // left win
        0xa3 => 0xe4,                                       // right ctrl
        0xa1 => 0xe5,                                       // right shift
        0xa5 => 0xe6,                                       // right alt
        0x5c => 0xe7,                                       // right win
        _ => return None,                                   // 0x10/0x11/0x12 (generic shift/ctrl/alt) are covered by the sided keys
    })
}

/// Inverse of [`vk_usage`].
pub fn usage_vk(usage: u8) -> Option<u16> {
    (0x08..=0xfeu16).find(|vk| vk_usage(*vk) == Some(usage))
}

fn extended(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2c..=0x2e | 0x5b | 0x5c | 0x6f | 0x90 | 0xa3 | 0xa5)
}

unsafe fn send(input: INPUT) {
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
}

fn key_input(vk: u16, up: bool) -> INPUT {
    let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
    if extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: INJECT_MARK } } }
}

fn mouse_input(flags: u32, data: i32, dx: i32, dy: i32) -> INPUT {
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: INJECT_MARK } } }
}

/// Play the events once. Delays are interruptible through `stop`.
pub fn play(events: &[Event], stop: &AtomicBool) {
    for e in events {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        unsafe {
            match e {
                Event::KeyDown(u) | Event::KeyUp(u) => {
                    if let Some(vk) = usage_vk(*u) {
                        send(key_input(vk, matches!(e, Event::KeyUp(_))));
                    }
                }
                Event::ButtonDown(b) | Event::ButtonUp(b) => {
                    let up = matches!(e, Event::ButtonUp(_));
                    let (flags, data) = match b {
                        1 => (if up { MOUSEEVENTF_LEFTUP } else { MOUSEEVENTF_LEFTDOWN }, 0),
                        2 => (if up { MOUSEEVENTF_RIGHTUP } else { MOUSEEVENTF_RIGHTDOWN }, 0),
                        3 => (if up { MOUSEEVENTF_MIDDLEUP } else { MOUSEEVENTF_MIDDLEDOWN }, 0),
                        4 => (if up { MOUSEEVENTF_XUP } else { MOUSEEVENTF_XDOWN }, 1),
                        5 => (if up { MOUSEEVENTF_XUP } else { MOUSEEVENTF_XDOWN }, 2),
                        _ => continue,
                    };
                    send(mouse_input(flags, data, 0, 0));
                }
                Event::Scroll(d) => send(mouse_input(MOUSEEVENTF_WHEEL, if *d > 0 { 120 } else { -120 }, 0, 0)),
                Event::Cursor(x, y) => send(mouse_input(MOUSEEVENTF_MOVE, 0, *x as i32, *y as i32)),
                Event::Delay(ms) => {
                    let mut left = *ms;
                    while left > 0 && !stop.load(Ordering::Relaxed) {
                        let step = left.min(50);
                        thread::sleep(Duration::from_millis(step as u64));
                        left -= step;
                    }
                }
                Event::Raw(_) => {}
            }
        }
    }
}

/// Split a macro into the steps of "Sequence" playback: one per press, cut at the delays.
pub fn sequence_steps(events: &[Event]) -> Vec<Vec<Event>> {
    let mut steps = Vec::new();
    let mut cur = Vec::new();
    for e in events {
        match e {
            Event::Delay(_) => {
                if !cur.is_empty() {
                    steps.push(std::mem::take(&mut cur));
                }
            }
            e => cur.push(e.clone()),
        }
    }
    if !cur.is_empty() {
        steps.push(cur);
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_mapping_roundtrip() {
        assert_eq!(vk_usage(0x41), Some(0x04));
        assert_eq!(vk_usage(0x5a), Some(0x1d));
        assert_eq!(vk_usage(0x30), Some(0x27));
        assert_eq!(vk_usage(0x70), Some(0x3a));
        assert_eq!(vk_usage(VK_F13), Some(0x68));
        assert_eq!(vk_usage(0xa0), Some(0xe1));
        assert_eq!(vk_usage(0x10), None);
        for vk in [0x41u16, 0x30, 0x0d, 0x20, 0x7c, 0x87, 0x25, 0xa2, 0xe0 - 0x40] {
            if let Some(u) = vk_usage(vk) {
                assert_eq!(usage_vk(u), Some(vk), "vk {vk:#x}");
            }
        }
    }

    #[test]
    fn steps_split_at_delays() {
        let ev = vec![Event::KeyDown(4), Event::KeyUp(4), Event::Delay(10), Event::KeyDown(5), Event::KeyUp(5), Event::Delay(5)];
        let s = sequence_steps(&ev);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1], vec![Event::KeyDown(5), Event::KeyUp(5)]);
    }
}
