//! Macro recorder: samples the keyboard and mouse buttons with
//! `GetAsyncKeyState` every few milliseconds and turns the changes into macro
//! events with the delays between them, like Synapse's macro recorder.

use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, GetCursorPos, WindowFromPoint};

use crate::macros::Event;
pub use crate::player::vk_usage;

/// Mouse button virtual keys: (vk, macro button code).
const MOUSE_VKS: [(u16, u8); 5] = [(0x01, 1), (0x02, 2), (0x04, 3), (0x05, 4), (0x06, 5)];

pub struct Recorder {
    pub events: Vec<Event>,
    started: Instant,
    last: Instant,
    down: [bool; 256],
}

impl Recorder {
    pub fn new() -> Recorder {
        let mut r = Recorder { events: Vec::new(), started: Instant::now(), last: Instant::now(), down: [false; 256] };
        // Keys already held when recording starts (the click on "Record") are not events.
        for vk in 1..=254u16 {
            r.down[vk as usize] = pressed(vk);
        }
        r
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    fn push(&mut self, e: Event) {
        let ms = self.last.elapsed().as_millis() as u32;
        if ms > 0 && !self.events.is_empty() {
            self.events.push(Event::Delay(ms));
        }
        self.events.push(e);
        self.last = Instant::now();
    }

    /// Sample the keys once. Clicks on the settings window itself (the Stop
    /// button) are not recorded; Esc is the stop key.
    pub unsafe fn poll(&mut self, own: HWND) {
        let on_window = {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            let h = WindowFromPoint(pt);
            h != 0 && GetAncestor(h, GA_ROOT) == own
        };
        for (vk, code) in MOUSE_VKS {
            let now = pressed(vk);
            let was = self.down[vk as usize];
            if now != was {
                self.down[vk as usize] = now;
                if on_window && code == 1 {
                    continue;
                }
                self.push(if now { Event::ButtonDown(code) } else { Event::ButtonUp(code) });
            }
        }
        for vk in 0x08..=0xfeu16 {
            let now = pressed(vk);
            let was = self.down[vk as usize];
            if now == was {
                continue;
            }
            self.down[vk as usize] = now;
            if vk == 0x1b {
                continue; // Esc stops the recording
            }
            let Some(usage) = vk_usage(vk) else { continue };
            self.push(if now { Event::KeyDown(usage) } else { Event::KeyUp(usage) });
        }
    }

    /// A wheel notch seen by the window.
    pub fn scroll(&mut self, up: bool) {
        self.push(Event::Scroll(if up { 1 } else { -1 }));
    }

    /// The recorded events without a leading delay and with every key released.
    pub fn finish(mut self) -> Vec<Event> {
        let mut held: Vec<Event> = Vec::new();
        for e in &self.events {
            match e {
                Event::KeyDown(k) => held.push(Event::KeyUp(*k)),
                Event::KeyUp(k) => held.retain(|h| *h != Event::KeyUp(*k)),
                Event::ButtonDown(b) => held.push(Event::ButtonUp(*b)),
                Event::ButtonUp(b) => held.retain(|h| *h != Event::ButtonUp(*b)),
                _ => {}
            }
        }
        for h in held.into_iter().rev() {
            self.events.push(h);
        }
        self.events
    }
}

fn pressed(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_mapping() {
        assert_eq!(vk_usage(0x41), Some(0x04));
        assert_eq!(vk_usage(0x5a), Some(0x1d));
        assert_eq!(vk_usage(0x30), Some(0x27));
        assert_eq!(vk_usage(0x70), Some(0x3a));
        assert_eq!(vk_usage(0xa0), Some(0xe1));
        assert_eq!(vk_usage(0x10), None);
        assert_eq!(crate::macros::key_name(0xe1).as_deref(), Some("shift"));
    }

    #[test]
    fn finish_releases_held_keys() {
        let mut r = Recorder {
            events: vec![Event::KeyDown(4), Event::Delay(10), Event::KeyDown(5), Event::KeyUp(4)],
            started: Instant::now(),
            last: Instant::now(),
            down: [false; 256],
        };
        r.last = Instant::now();
        let ev = r.finish();
        assert_eq!(ev, vec![Event::KeyDown(4), Event::Delay(10), Event::KeyDown(5), Event::KeyUp(4), Event::KeyUp(5)]);
    }
}
