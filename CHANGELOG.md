# Changelog

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
