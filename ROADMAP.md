# freenapse: feature status and roadmap

What freenapse does today, what it does only partly, what it does not do yet,
and what the next releases aim for. The protocol details behind each line are
in [FEATURES.md](FEATURES.md). The settings window shows the same list on its
ROADMAP tab.

Legend: ✅ done and verified on the mouse, 🟡 partial, ❌ not yet, 🚫 out of scope.

## Status

### Performance

| Feature | Status | Notes |
|---|---|---|
| DPI stages (up to 5), active stage | ✅ | CLI, config, settings window |
| Live DPI 100..35000, 1 DPI steps | ✅ | |
| Independent X/Y DPI | ✅ | CLI (`dpi x y`), config (`y_stages`), window toggle "Independent X and Y" |
| Polling rate 125..8000 Hz | ✅ | 8000 needs the HyperPolling dongle |
| In-game polling rate | ✅ | switches while a game with a profile owns the foreground window |
| Sensor rotation (angle tune) | ✅ | |
| Dynamic Sensitivity (acceleration curve) | ✅ | Classic / Natural / Jump presets and a custom curve editor; the curve is written to the mouse and read back |
| Sensitivity Matcher | 🚫 | software-only Synapse helper; set the DPI you want |

### Calibration

| Feature | Status | Notes |
|---|---|---|
| Smart Tracking 1/2/3 mm | ✅ | verified on the mouse |
| Asymmetric cut-off (lift-off 2..26, landing 1..25) | ✅ | written exactly as Synapse's engine for this mouse does (lift mode 4 + two level bytes), every step answers Success |
| Razer mouse-mat presets, custom surface calibration, mini calibration | 🚫 | Synapse loads the `orca5PlusCalibrationEngine` for the Viper V3 Pro, which has only Smart Tracking and the asymmetric cut-off; the surface-calibration commands of its other engines are refused by this firmware (tested wired, wireless, driver mode, with the left button held) |
| Read back the current asymmetric levels | 🟡 | the configuration register does not return them, so `tracking = "keep"` leaves them untouched and `apply` always rewrites them |

### Power

| Feature | Status | Notes |
|---|---|---|
| Battery level and charging state | ✅ | tray icon, window header, CLI |
| Low battery desktop notification | ✅ | threshold in config and window |
| Idle sleep timer 1..15 min | ✅ | |
| Low power mode threshold 5..100 % | ✅ | firmware honours it only at 1000 Hz and below, same as Synapse |
| HyperPolling dongle LED mode | ✅ | |

### Customize

| Feature | Status | Notes |
|---|---|---|
| Button remapping, 7 buttons, normal + Hypershift layer | ✅ | onboard profile 1 |
| Mouse functions, keyboard keys with modifiers, media keys, DPI stage / cycle, Hypershift, disabled | ✅ | |
| Double-click, turbo (mouse button or key, with repeat rate), system keys sleep / power / wake | ✅ | config and window, verified read-back |
| Profile switch button | 🚫 | the mouse reports a single onboard profile, so there is nothing to switch to |
| Macros | ✅ | recorded in the window (keys, mouse buttons, wheel, delays) or written as steps in `[[macros]]`; five playback options. The Viper V3 Pro has no macro memory (every macro command answers "not supported" and Synapse's spec for it says `macros: false`), so, like Synapse, the tray app plays them: the button sends F13..F24 and the tray's keyboard hook turns it into the macro. Mice with macro memory get them stored onboard with Synapse's commands |
| Software bindings (launch program, type text, Windows shortcuts) | ❌ | needs a resident input hook |

### App and device

| Feature | Status | Notes |
|---|---|---|
| Settings window (game-client layout: navigation pane, settings-row cards, cover tiles; DESIGN.md), immediate apply | ✅ | pending while the mouse sleeps, written when it wakes |
| Tray icon: battery, DPI stage, polling rate menu | ✅ | |
| Config file, import from the mouse, re-apply | ✅ | `%APPDATA%\freenapse\config.toml` |
| Start at login | ✅ | registry Run key, toggle in the window's gear page |
| Firmware versions and serial numbers | ✅ | mouse and dongle |
| Onboard profile selection 1..5 | ✅ | `profile.active` in the config, `freenapse profile <n>` / `profiles`, selector on the gear page; buttons and Dynamic Sensitivity go to that profile. The Viper V3 Pro answers 1 to "max profiles supported" (`0x05/0x8a`) and refuses to create or activate others, so it accepts 1 only |
| Per-game profile switching | ✅ | `[[games]]`: DPI stage, polling rate and button layout per game, switched by the tray app with the foreground window; GAMES tab, and a game layer on the Customize tab |
| Game library | ✅ | installed games read from Steam, Epic Games, GOG, EA app, Battle.net, Riot, Ubisoft Connect, Rockstar and the Xbox app (local files and registry, no account); any other program through a file dialog |
| Dongle pairing | ✅ | pair, cancel, unpair (confirmed twice) and auto-pairing on the gear page and in the CLI, Synapse's command bytes |
| Bluetooth | 🚫 | the Viper V3 Pro has no Bluetooth |
| Other Razer mice | ❌ | only the Viper V3 Pro product ids are recognised |
| Firmware update | 🚫 | use Razer's updater once if you need it |
| Razer account, cloud profiles, telemetry, Chroma | 🚫 | by design; the Viper V3 Pro has no lighting |

## Roadmap

No dates. Each release ships when the items are verified on the mouse.

### Next release

* Onboard macros: record in the window, store in the mouse's macro memory, bind to a button.
* Per-game profiles: reuse the foreground-window detection from the in-game polling rate to switch DPI stages and button layouts.
* Dongle pairing and unpairing from the gear page.

All three are implemented. How each one turned out on the hardware:

* Macros: the Viper V3 Pro has no macro memory (every macro command answers "not supported", Synapse's spec says `macros: false`), so the macro is played by the tray app, exactly as Synapse does for this mouse. "Record new macro" on the Customize tab captures keys, mouse buttons, wheel notches and delays; the binding gets one of the keys F13..F24 in the mouse and the tray's keyboard hook turns it into the macro, with Play once / N times / While held / Toggle / Sequence. Verified end to end: a macro bound to the Back button and applied to the mouse; a physical press of Back typed the macro's text. Mice with macro memory get the macro stored onboard with Synapse's commands (`freenapse macros`, `macro-read`, `macro-write`, `macro-delete`).
* Per-game profiles: `[[games]]` entries with a DPI stage, a polling rate and a button layout per program; the Performance tab edits them, the Customize tab has a layer per game. Verified: with Notepad as the "game", two seconds after it took the foreground the mouse was at the game's polling rate with the game's button layout, and both were restored after it closed.
* Dongle pairing: pair a device, cancel, unpair (asks twice) on the gear page, plus `freenapse pair`, `pair-cancel`, `unpair --yes`, `pair-status`. Verified on the dongle: the paired-device list, pairing mode start (answer 1) and cancel (answer 4) with the mouse staying paired. Unpair was not run: it disconnects the only mouse until it is re-paired by hand, and the owner chose to skip that test. Auto-pairing is not supported by this dongle (no answer), so its toggle stays hidden.

### Previous release

* Double-click, turbo and system keys; Dynamic Sensitivity with the curve editor; independent X/Y DPI; onboard profile selection (the Viper V3 Pro accepts profile 1 only); Calibration tab rebuilt on Synapse's engine for this mouse (no surface calibration exists for it).

### Later

* Software bindings that need a resident input hook: launch a program, type text, Windows shortcuts.
* A product table so other Razer mice that share this protocol can be added without code changes.
* Bluetooth connection support for mice that have it.
* Code-signing the executable.

### Done since

* Single portable executable: `freenapse.exe` is the tray app when started without arguments and the CLI otherwise; a `config.toml` next to it is used in preference to `%APPDATA%`. It carries its own icon.
* Synapse uninstalled on the development PC: freenapse kept driving the mouse (status, apply, polling, buttons, tray) with no Razer software, service or startup entry left.
* GAMES tab: the game list fills itself from Steam, Epic Games, GOG, EA app, Battle.net, Riot, Ubisoft Connect, Rockstar and the Xbox app (35 games found on the development PC: Steam, Epic, EA app, Battle.net, Xbox); click a game to create its profile, "Add a program..." for anything else. Profiles of launcher games match their whole install folder, so a game's launcher and 64-bit sub-binaries count too.

### Not planned

Razer account, cloud profile sync, telemetry, firmware updates and
Sensitivity Matcher. The point of freenapse is to do without them.
