# Changelog

## Unreleased

- Program and Text buttons: a button starts a program (or opens a file or URL)
  or types a text with any characters in any layout. They are one-step macros
  (`launch:`, `text:`) that the tray app plays like the other software macros;
  the Customize page's Program and Text functions create them. Windows
  shortcuts need nothing new: `key:win+d` is an onboard key.
- The tray reconnects as soon as the dongle is plugged in or the PC resumes
  from sleep (device and power notifications) instead of at the next poll.
- The tray watches the config file and writes it to the mouse when it is edited
  by hand; no `freenapse apply` needed.
- Update notice: once a day the tray asks GitHub for the latest release and,
  when it is newer, says so once and adds "Update to freenapse x.y.z..." to
  its menu. One GET to api.github.com, nothing about you in it;
  `[tray] check_updates = false` turns it off.
- Packaging: winget manifests (`MatteoSantoro.freenapse`, portable) and a
  Scoop manifest in `packaging/`, both ways around the SmartScreen prompt an
  unsigned download gets.
- CI runs rustfmt and clippy (`-D warnings`); `tests/golden.rs` freezes the
  exact bytes of every command builder.

## 0.2.0 (2026-10-30)

- A device table (`src/device.rs`, `DeviceSpec`) drives what freenapse does per
  product id: transaction id, polling command, polling rates, DPI maximum,
  sensor tracking page, dongle pairing. The Viper V3 Pro rows carry exactly the
  previous behaviour; the other rows take their facts from OpenRazer's device
  list and are unverified.
- Any Razer mouse with the 90-byte control report now opens. One that is not in
  the table, or is there unverified, is read-only: `status` and `probe` work,
  every write is refused with a message that says how to help. `[device]
  allow_unverified = true` in the config lifts that at the owner's risk.
- `freenapse probe`: every read the mouse answers, raw, as a code block to
  paste into an issue so a table row can be written for a new mouse.
- The settings window hides the Calibration page and the dongle pairing and
  LED controls on mice where the table does not know them.
- `FEATURES.md` lists the supported mice with who verified them;
  `CONTRIBUTING.md` describes how to add one.
- `freenapse version`; the version is also in `help`, the tray tooltip and the
  gear page of the settings window.
- A crash writes `crash.txt` next to the config file (version, message, source
  location, Windows build). The tray and the settings window say so in a
  message box before they stop.
- `--log` or `--log=<file>` on any command, and on the tray, records every
  exchange with the mouse (command, transaction id, timing, result; no input)
  in `hid.log` next to the config file. The `FREENAPSE_HIDLOG` environment
  variable it replaces still works.
- Issue templates for a bug and for adding a mouse.

## 0.1.0 (2026-10-28)

First release, for the Razer Viper V3 Pro on Windows 10/11, wired or through
the HyperSpeed / HyperPolling dongle. One executable of about 1 MB: no
installer, no services, no account.

**Stored in the mouse**

- DPI: up to 5 stages, active stage, independent X and Y, 100 to 35000 in steps of 1
- Polling rate 125 to 8000 Hz (4000 and 8000 need the HyperPolling dongle)
- Button assignment on the normal and Hypershift layers: mouse buttons, double-click, keyboard keys with modifiers, DPI stage and cycle, media keys, system sleep / power / wake, macros, Hypershift, disabled; Turbo with a repeat rate
- Dynamic Sensitivity: Classic, Natural, Jump or your own curve
- Sensor: Smart Tracking 1 to 3 mm or the asymmetric cut-off with lift-off and landing levels; angle correction
- Power: idle sleep timeout, low power mode threshold; HyperPolling dongle indicator LED
- Dongle pairing: pair, cancel, unpair; firmware versions and serial numbers

**Tray app**

- Battery percentage and charging state; live DPI stage and polling rate switching
- Per-game profiles (DPI stage, polling rate, button layout) switched with the foreground window; games found automatically in Steam, Epic, GOG, EA app, Battle.net, Riot, Ubisoft Connect, Rockstar and the Xbox app
- In-game polling rate: a higher rate only while a game is running
- Macros recorded in the window or written in the config, played once, N times, while held, as a toggle or as a sequence (the Viper V3 Pro has no macro memory, so the tray app plays them, like Synapse does)
- Start at login

**Settings window and CLI**

- Native Win32/GDI window with the Customize, Performance, Calibration, Power, Games and Roadmap pages, plus app settings with the onboard profile selector and dongle pairing
- `freenapse status`, `config`, `apply`, `import` and one command per setting; a TOML config file in `%APPDATA%\freenapse` or next to the executable for portable use

**Good to know**

- Tested on one mouse. Other Razer mice are not supported yet.
- Do not keep Synapse running alongside: it rewrites the settings it thinks it owns within seconds.
- Use at your own risk; see NOTICE.md for trademarks and provenance.
