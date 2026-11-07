# Changelog

## Unreleased

- Fix: started at login, the tray app could run without its icon. Right
  after sign-in the taskbar often refuses new icons, and the tray never
  tried again: it lived in a message-only window, which does not receive
  the TaskbarCreated broadcast. The window is now a hidden top-level one,
  and a refused icon is retried every two seconds until the taskbar takes
  it. The icon also comes back after Explorer restarts.

## 0.4.1 (2026-11-05)

- Devices page in the settings window: every mouse of the device table,
  grouped into verified, being tested and read-only, with product ids, DPI
  ceiling, polling rates and the connected mouse marked.
- The About section names the verified mice and links the website, source
  and license; the Roadmap no longer says only the Viper V3 Pro is
  recognised and lists Chroma lighting as partial.

## 0.4.0 (2026-11-05)

- Razer Basilisk V2 (product id 0x0085), verified on the author's mouse:
  DPI stages up to 20000, polling, onboard macros and all 13 buttons. That is
  the Viper's seven plus DPI up and down, the two wheel tilts, the
  sensitivity clutch and the profile button, each with its factory default.
  The Customize page lists and draws the buttons the connected mouse has.
- Lighting for mice with LEDs: off, static, breathing (one colour or random)
  or spectrum per zone, and one brightness, stored in the mouse. The Basilisk
  V2 has a logo and a wheel zone. New `[lighting]` config table,
  `freenapse lighting` command and a Lighting section on the Performance page.
  A config without the table leaves the lights alone.
- New button functions: `clutch:<dpi>` (sensitivity clutch) and
  `profile-next`, `profile-prev`, `profile-cycle-up`, `profile-cycle-down`.
- Each verified mouse can keep its own config file. The Basilisk V2 uses
  `config-basilisk-v2.toml`, made from its own settings the first time, and
  the Viper keeps `config.toml`. The tray switches files when another mouse
  connects, so one mouse's settings are never written to the other.
- Mice without a battery: status, apply, the tray and the window skip battery,
  idle sleep and low battery, show "wired", and hide the Power page when
  there is no dongle either.
- Fix: the macro id list is read with `0x06/0x8b`. The old `0x06/0x81`
  answered zeros on a mouse with macro memory, so import missed the stored
  macros and dropped every button bound to one.

## 0.3.1 (2026-11-05)

- `freenapse probe` scans every button code (0x01 to 0x7f) and prints what
  each button that answers does now, so a mouse with more buttons than the
  Viper shows its layout in one paste. On the Viper V3 Pro it finds the DPI
  button under the mouse at 0x60. The probe now takes about half a minute and
  says so when it starts.
- The Razer Pro Click V2 Vertical through its HyperSpeed dongle (product id
  0x00C8) is in the device table, from the probe its owner posted: classic
  polling, 30000 DPI, no Dynamic Sensitivity or rotation. It is read-only
  until its owner confirms writes with `allow_unverified`.
- CONTRIBUTING.md asks owners to quit Synapse completely before testing
  writes, since it puts its own profile back.

## 0.3.0 (2026-10-31)

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
- Experimental support for the wired DeathAdder V3 Pro (product id 0x00B6),
  contributed by kyvg after probing the mouse: its 30000 DPI limit, and the
  Dynamic Sensitivity and rotation commands it rejects are skipped and hidden.
  It stays behind the `allow_unverified` gate.
- The settings window tells a sleeping mouse (kept pending, applied on wake)
  from a mouse that rejected a write (reported, not retried). Re-importing the
  config keeps the owner's `allow_unverified` choice.

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
