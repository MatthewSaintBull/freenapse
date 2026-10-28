# Razer Synapse 4 feature map for the Viper V3 Pro, and what freenapse does with it

Sources: the behaviour of Synapse 4 with the Viper V3 Pro as observed on one
machine (studied for interoperability only; freenapse contains no Razer code,
files or assets), the GPL OpenRazer driver, and the razerqdhid protocol notes.
Everything marked *verified* was exercised against the real mouse through the
HyperPolling dongle.

## What Synapse costs

| | Synapse 4 (measured) |
|---|---|
| Processes | 11 x `RazerAppEngine.exe` (Electron) + `razer_elevation_service` + `Razer Game Manager Service 3` |
| Working set | ~1.5 GB total (largest process 260 MB) |
| CPU | the "background-manager" renderer polls the device and runs a game-detection engine continuously |
| Storage | 220 MB Electron runtime plus per-product modules downloaded on demand |

freenapse: one `freenapse.exe` (~0.9 MB: tray icon, settings window and CLI), no services, no installer, no runtime.

### The settings window

`freenapse.exe` carries a settings window with the controls of Synapse's mouse
page in a game-client layout: a navigation sidebar (Customize / Performance /
Calibration / Power / Games / Roadmap, Settings, and a device card with the
battery), a page header, and flat cards of settings rows following the
Windows 11 / Fluent 2 guidance written down in DESIGN.md (tokens, type ramp,
spacing, states).
It is pure Win32 + GDI (cards, toggles, segmented chips, sliders, the game
tiles and the mouse drawing are painted by hand with GradientFill / AlphaBlend
for the subtle gradients, the tile hover shadow and the battery reflection; icons are glyphs of Segoe Fluent
Icons; game covers are decoded with GDI+; only the text boxes are real EDIT
controls), dark themed, per-monitor DPI aware. Like Synapse it has no apply
button: each control writes its setting to the mouse (and to the config file)
the moment it changes; while the mouse is asleep the change stays pending and
is pushed when it wakes. Open it by clicking the tray icon, with
`freenapse ui`, or by launching the tray executable a second time.

## Feature map

Legend: ✅ replicated, 🟡 partially / different approach, ❌ not replicated.
Status at a glance and the release roadmap: [ROADMAP.md](ROADMAP.md).

### Performance (Synapse "Performance" tab)

| Synapse feature | Where it lives | freenapse | Command |
|---|---|---|---|
| Sensitivity stages (up to 5), active stage | onboard | ✅ verified read | `0x04/0x06` set stages, `0x04/0x86` get (VARSTORE, 7 bytes per stage) |
| Live DPI X/Y, 100..35000 in 1 DPI steps | onboard | ✅ | `0x04/0x05` / `0x04/0x85` |
| Independent X/Y sensitivity | onboard | ✅ CLI `dpi x y`, config `dpi.y_stages`, window toggle | same |
| Polling rate 125..8000 Hz | onboard | ✅ verified read (1000 Hz) | `0x00/0x40` set (sent twice, arg 0x00 then 0x01), `0x00/0xC0` get. Codes 8000=0x01 4000=0x02 2000=0x04 1000=0x08 500=0x10 250=0x20 125=0x40 |
| In-game polling rate (boost while a game runs) | software | ✅ tray app switches while a game with a profile owns the foreground window; the game list is filled from the launchers (see below) | config `polling.game_rate`, `[[games]]` |
| Dynamic Sensitivity (acceleration curve) | onboard | ✅ verified: state and mode read back, a custom curve written in 32 packets reads back exactly | `0x0b/0x10` state `[1, on]`, `0x0b/0x11` mode `[1, 0..3]`, `0x0b/0x13` curve `[1, mode, 250, offset, count, 8 x f32 LE]`; Synapse's order: state, then mode, then the curve (15 ms between packets). Presets Classic 0 / Natural 1 / Jump 2 are in the firmware; Custom 3 takes 250 multipliers indexed by speed 1..250, computed from control points with Synapse's monotone cubic spline (`src/accel.rs`) |
| Rotation (angle tune) | onboard | ✅ verified read (`[01, 00, f6]` = off, -10°) | `0x0b/0x14` `[1, enabled, degrees as i8]`, get `0x0b/0x94 [1]` |
| Sensitivity Matcher (match another mouse's DPI) | software only | ❌ (set the DPI you want) | |

### Calibration (Synapse "Calibration" tab)

| Synapse feature | freenapse | Command |
|---|---|---|
| Smart Tracking, symmetric 1/2/3 mm | ✅ | `0x0b/0x03` state `[0, 4, 0]`, then `0x0b/0x0b` lift setting `[0, 4, mode=1, height=mm-1]` |
| Asymmetric Cut-off (lift-off 2..26, landing 1..25, Synapse default 13 / 12) | ✅ every step answers Success; the same sequence Synapse 4 sends for this mouse | state `[0, 4, 1]`, lift `[0, 4, mode=4, 0]`, then `0x0b/0x05` config `[0, 4, lift-off - 1, landing - 1]` |
| Razer mouse-mat presets, custom surface calibration, mini calibration | 🚫 not offered by Synapse 4 for the Viper V3 Pro. The commands it uses for other mice (`0x0b/0x09` control, `0x0c`/`0x0d` tables) answer status 0x03 on firmware 1.11, wired, wireless, in driver mode and with the left button held | |

All sensor commands start with class id 0 and PixArt sensor id 4 (`CALIBRATION_SENSOR_ID.Avago_Proximity_sensor` in Synapse).

### Power (Synapse "Power" tab)

| Synapse feature | freenapse | Command |
|---|---|---|
| Battery level, charging state | ✅ verified (37% matched Synapse) | `0x07/0x80` → byte 1 on a 0..255 scale; `0x07/0x84` → byte 1 |
| Idle sleep timer ("powerSavingValue", minutes in the UI) | ✅ seconds 60..900 | `0x07/0x03` big-endian seconds, get `0x07/0x83` |
| Low power mode threshold ("When wireless, enter Low Power Mode if the battery level is below (%)") | ✅ 5..100 %; like Synapse the control is greyed out above 1000 Hz because the firmware ignores it there | `0x07/0x01` byte on 0..255 scale, get `0x07/0x81`; current state `0x07/0x95` |
| Battery notifications on the desktop | ✅ tray balloon at `tray.notify_percent` | |
| Wireless dongle LED (`indicatorLedStatus`) | ✅ | `0x07/0x10` to transaction id 0xff (the dongle), get `0x07/0x90` |

### Customize (Synapse "Customize" tab)

| Synapse feature | freenapse | Command |
|---|---|---|
| Button remapping, 7 inputs x 2 layers (normal / Hypershift) | ✅ onboard profile 1, verified read | `0x02/0x0c` `[profile, button, hypershift, category, len, data[5]]`, get `0x02/0x8c` |
| Mouse functions, keyboard keys + modifiers, media keys, DPI stage/cycle, Hypershift | ✅ | categories 0x01, 0x02, 0x0a, 0x06, 0x0c |
| Double-click, turbo, sleep/power/wake keys | ✅ verified read-back; config `double-click`, `turbo:<button>:<per second>`, `turbo-key:<combo>:<per second>`, `system:sleep|power|wake` | 0x0b `[01]`; 0x0e `[button, delayHi, delayLo]` with delay = 1000 / rate ms; 0x0d `[mods, key, delayHi, delayLo]`; 0x09 `[0x82 sleep, 0x81 power, 0x83 wake]` |
| Profile switch button | 🚫 decoded by `buttons` (0x07: 1 next, 2 previous, 3 + id, 4/5 cycle) but pointless: the mouse holds one profile | 0x07 |
| Macros | ✅ recorded in the window or written as steps in `[[macros]]`; bound with `macro:<id>[:<n>]`, `macro-held:<id>`, `macro-toggle:<id>`, `macro-seq:<id>`. The Viper V3 Pro has no macro memory: every class 0x06 command answers 0x05 (not supported) and Synapse's device spec says `OBMSpecs.supportedFeature.macros: false`, so Synapse plays its macros in software. freenapse does the same: each distinct binding gets a key F13..F24 (HID 0x68..0x73) in the mouse and the tray app's `WH_KEYBOARD_LL` hook swallows it and plays the events with `SendInput` | software on this mouse; for mice with macro memory: class 0x06 (see below) and button categories 0x03 `[idHi, idLo, times]`, 0x04 held, 0x05 toggle, 0x0f sequence `[idHi, idLo]` |
| Software-side bindings (launch program, text, Windows shortcuts) | ❌ these need a resident input hook | |
| Scroll settings | n/a, the Viper V3 Pro has a fixed notched wheel | |

### Profiles, lighting, other

| Synapse feature | freenapse |
|---|---|
| Onboard profiles 1..5, profile selection | ✅ `profile.active`, `freenapse profile <n>`, gear page: creates the slot (`0x05/0x02`) when missing and activates it (`0x05/0x04`), then every button / acceleration command carries that id. The Viper V3 Pro reports 1 existing profile and a maximum of 1 (`0x05/0x80`, `0x05/0x8a`; list `0x05/0x81` = `[1, 1]`) and answers Failure to create, activate and get-active, so it accepts profile 1 only |
| Cloud profiles | ❌ one config file |
| Per-game automatic profile switching | ✅ `[[games]]`: DPI stage, polling rate and a button layout per game, written by the tray app while that game owns the foreground window and reverted when it ends (software-side, like Synapse). A game is matched by its install folder (`dir`: any program started from inside it) or by its program name (`exe`) |
| Game library | ✅ local scan, no account: Steam (`libraryfolders.vdf` + `appmanifest_*.acf`), Epic Games (`ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item`, games only, no DLC), GOG Galaxy (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games`), EA app / Battle.net / Riot / Ubisoft Connect / Rockstar (their uninstall entries, by publisher, launchers excluded), Xbox app (`X:\XboxGames\*\Content\MicrosoftGame.config` on fixed drives); one entry per install folder, shown as cover tiles (Steam `appcache\librarycache` art, Xbox `MicrosoftGame.config` logos, otherwise the program's icon). Other programs: "Add a program..." file dialog. `src/library.rs`, `src/ui/art.rs`, `freenapse games` |
| Chroma lighting | n/a, the mouse has no RGB |
| Firmware update | ❌ use Razer's updater once if needed |
| Firmware version, serial number (mouse and dongle) | ✅ verified: dongle fw 1.11, serials |
| Dongle pairing / unpairing | ✅ gear page and `freenapse pair / pair-cancel / unpair --yes / pair-status`, with Synapse's bytes: pairing mode `0x00/0x41` `[1 start / 4 cancel, pidHi, pidLo]`, unpair `0x00/0x42` `[pidHi, pidLo]`, scan `0x00/0x46` `[mode]`, auto-pairing `0x00/0x55` set / `0xd5` get, paired list `0x00/0xbf` `[count, (state, pidHi, pidLo)...]`, all addressed to the dongle |
| Bluetooth | n/a, the Viper V3 Pro has no Bluetooth |
| Razer account, telemetry, Razer Central | ❌ by design (the game library is read locally from the launchers instead) |

## Protocol reference

90-byte feature report on HID interface 0 (`mi_00`, usage page 1 usage 2). On
Windows the buffer is 91 bytes: report id 0 + report.

```
offset  0  status            request: 0x00. response: 01 busy, 02 ok, 03 failure, 04 timeout, 05 not supported
        1  transaction id    0x1f = mouse (via dongle), 0xff = dongle itself
        2  remaining packets (u16 BE, 0)
        4  protocol type     0
        5  data size         number of argument bytes
        6  command class
        7  command id        bit 7 = read
        8  arguments[80]
       88  crc               XOR of bytes 2..87
       89  reserved
```

Timing, as measured here:

* Wait ≥150 ms between `HidD_SetFeature` and `HidD_GetFeature`. Reading earlier returns status 0x04 and the exchange is lost (OpenRazer waits 60 ms on Linux; through this dongle on Windows that was not enough).
* The wireless mouse stops answering when it has been idle for a while (status 0x04 from the dongle), even though Synapse keeps showing it as connected. In one session it went silent within seconds of the last input, in another it kept answering for 30 s of idle. Synapse gets its updates from hardware-event input reports (report id 5 on the vendor collection) instead of polling. freenapse treats 0x04 as "asleep", keeps the last known values, and retries writes when `GetLastInputInfo` shows fresh input.
* The parameter getters `0x0b/0x8c`, `0x0b/0x8d` and `0x0b/0x86` answered status 0x03 (failure) on this firmware (1.11); the setters are what Synapse uses. The sensor config getter `0x0b/0x85` answered but with a payload that matched no table while a mat preset was active, so `freenapse import` falls back to `tracking = "keep"` rather than guessing.
* Synapse and freenapse can share the dongle; each response is matched against its request (class, id, transaction id) and resent if another client's reply shows up.

Command table (class / id / size / argument bytes):

| Purpose | Get | Set | Args |
|---|---|---|---|
| Firmware version | `00/81/02` | | → major, minor |
| Serial | `00/82/16` | | → ASCII |
| Device mode | `00/84/02` | `00/04/02` | mode (0 normal, 3 driver), 0 |
| Polling (HyperPolling) | `00/C0/01` | `00/40/02` | arg, code (see above) |
| Polling (wired legacy) | `00/85/01` | `00/05/01` | 1=1000 2=500 8=125 |
| DPI X/Y | `04/85/07` | `04/05/07` | varstore, xH, xL, yH, yL, 0, 0 |
| DPI stages | `04/86/26` | `04/06/26` | varstore, active, count, then per stage: idx, xH, xL, yH, yL, 0, 0 |
| Battery | `07/80/02` | | → [_, level] |
| Charging | `07/84/02` | | → [_, charging] |
| Idle sleep | `07/83/02` | `07/03/02` | seconds BE |
| Low battery | `07/81/01` | `07/01/01` | level 0..255 |
| Dongle LED | `07/90/01` (tid ff) | `07/10/01` (tid ff) | 1..3 |
| Button | `02/8c/0a` | `02/0c/0a` | profile, button, hypershift, category, len, data[5] |
| Sensor state | `0b/83/03` | `0b/03/03` | 0, 4, state |
| Lift setting | `0b/8b/04` | `0b/0b/04` | 0, 4, mode, height |
| Sensor config | `0b/85/0b` | `0b/05/0b` | 0, 4, bytes (asymmetric cut-off: lift-off - 1, landing - 1); the getter does not return the levels |
| Angle tune (rotation) | `0b/94/03` | `0b/14/03` | 1, enabled, degrees |
| Acceleration state | `0b/90/02` | `0b/10/02` | profile (1), on |
| Acceleration mode | `0b/91/02` | `0b/11/02` | profile, mode (0 classic 1 natural 2 jump 3 custom) |
| Acceleration curve setting | `0b/92/03` | `0b/12/03` | profile, mode, input offset (unused by Synapse's task runner) |
| Acceleration curve data | `0b/93/25` | `0b/13/25` | profile, mode (3), total points (250), point offset, point count (<= 8), 8 x f32 little-endian |
| Profile count / max / list | `05/80/01`, `05/8a/01`, `05/81/50` | | → count; → max; → [count, ids...] |
| Active profile / create / delete | `05/84/01` | `05/04/01`, `05/02/01`, `05/03/01` | id (all four answer Failure on the Viper V3 Pro, which holds one profile) |
| Macro storage info | `06/8e/0e` | | → maxId u16, total u32, available u32, recyclable u32 (bytes) |
| Macro count / id list | `06/80/02`, `06/81/50` | | → count u16; → [offset u16, count u16, id u16...] |
| Macro allocate / delete | | `06/08/06`, `06/03/02` | id u16, size u32; id u16 |
| Macro name structure | `06/84/50` | `06/04/<6+n>` | id u16, offset u16, total u16 (250), bytes (64 per packet): GUID[16], item count u32 LE, name[100], 0[16], timestamp[16], hash[32], 0[66] |
| Macro data | `06/89/50`, size `06/88/06` | `06/09/<7+n>` | id u16, offset u32, count, bytes (72 per packet) |
| Dongle pairing mode | | `00/41/03` (tid ff) | 1 start / 4 cancel, pid u16. Verified: start answers `[01, 00c1]`, cancel `[04, 00c1]`, the mouse stays paired |
| Dongle unpair / scan | | `00/42/02`, `00/46/01` (tid ff) | pid u16; scan mode |
| Dongle auto-pairing / paired list | `00/d5/01`, `00/bf/50` (tid ff) | `00/55/01` | on; → [count, (state, pid u16)...]. Verified on the HyperPolling dongle: `[02, 00 00c1, 00 ffff]` = the Viper V3 Pro (state 0 while asleep, 1 when connected) and an empty slot. The auto-pairing getter is not answered by this dongle (timeout) and the mouse says not supported, so the toggle stays hidden |
| Dongle primary device | `00/c5/03` (tid ff) | | → [count, pid u16]; verified `[01, 00c1]` |

Macro event bytes (Synapse's onboard macro format): `01 usage` key down, `02
usage` key up, `08 mask` mouse buttons held (bit 0 left .. bit 4 forward), `0a
01` wheel up / `0a ff` wheel down, `11 ms` / `12 ms(u16)` / `13 ms(u24)` / `14
ms(u32)` delay, `15 dx(i16) dy(i16)` cursor move. A macro needs
`ceil((len + 256) / 256) * 256` bytes of macro memory.

Button codes: left 01, right 02, middle 03, back 04, forward 05, scroll up 09,
scroll down 0a. Function categories: 00 disabled, 01 mouse button, 02 key
(modifier bitmask, HID usage), 0a consumer usage (BE), 06 DPI (01 up, 02 down,
03 + stage), 07 profile (01 next, 02 previous, 03 + id, 04/05 cycle), 0c
Hypershift, 0b double click (`[01]`), 0e turbo button (button, interval ms BE),
0d turbo key (modifiers, usage, interval ms BE), 09 system key (81 power, 82
sleep, 83 wake). Synapse computes the turbo interval as 1000 / presses per
second.

