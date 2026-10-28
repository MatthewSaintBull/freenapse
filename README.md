<p align="center"><picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-wordmark.svg">
  <img src="assets/logo-wordmark-light.svg" alt="freenapse" width="360" height="80">
</picture></p>

# freenapse

A tiny Rust replacement for Razer Synapse, built for the **Razer Viper V3 Pro**
(wired, or through the HyperSpeed / HyperPolling dongle).

Synapse 4 on this machine runs as 11 Electron processes using about 1.5 GB of RAM
and burns CPU while gaming. freenapse is one small native executable with no
runtime dependencies, no installer and no services:

| | Synapse 4 | freenapse |
|---|---|---|
| Processes | 11 (RazerAppEngine) + 2 services | 1 tray process (or none: run the CLI once) |
| Memory | ~1.5 GB | 10 MB working set (measured) |
| Background CPU | constant | idle; a battery read once a minute |
| Needs an account / internet | yes | no |

Everything freenapse sets is written to the mouse's **onboard memory**, so the
settings survive reboots and work on any computer with nothing running at all.
The tray app only adds live battery display, quick switching and the in-game
polling rate.

freenapse is an independent open-source project. It is not affiliated with,
endorsed by or supported by Razer Inc. Razer, Razer Synapse and Viper are
trademarks of Razer Inc., named here only to say which hardware and software
this project works with. The protocol was worked out for interoperability
from the mouse's own answers, the GPL OpenRazer driver and public protocol
notes; the program contains no Razer code, files or assets.

**Use at your own risk.** freenapse changes the settings stored in the mouse's
onboard memory through the same HID feature reports Synapse uses. It cannot
read or modify firmware, and every setting can be written again, but the
authors give no warranty and Razer will not support a mouse configured this
way. To go back, delete the config file and run `freenapse apply` for
freenapse's defaults, or reinstall Synapse, which rewrites every setting from
its own profile.

## What it does

* DPI: up to 5 stages, active stage, live DPI (100..35000, 1 DPI steps), independent X/Y
* Dynamic Sensitivity: the onboard acceleration curve (Classic, Natural, Jump, or your own curve)
* Polling rate: 125..8000 Hz through the dongle (8000 needs the HyperPolling dongle)
* In-game polling rate: automatically switch to a higher rate while a game is in the foreground
* Power: idle sleep timeout (60..900 s), low power mode threshold (5..100 %, honoured by the firmware at 1000 Hz and below), battery level and charging state
* Sensor: Smart Tracking 1..3 mm and the asymmetric cut-off (lift-off and landing levels), written exactly as Synapse does for this mouse
* HyperPolling dongle indicator LED mode
* Onboard button remapping (profile 1, normal and Hypershift layer): mouse buttons, double-click, turbo, keyboard keys with modifiers, media keys, DPI stage/cycle, system sleep / power / wake, macros, Hypershift
* Macros: record keys, mouse buttons, wheel and delays in the window (or write the steps in the config); played once / N times / while held / toggle / as a sequence. The Viper V3 Pro has no macro memory, so the tray app plays them, as Synapse does for this mouse (mice with macro memory get them stored onboard)
* Game library and per-game profiles: the games installed with Steam, Epic Games, GOG, EA app, Battle.net, Riot, Ubisoft Connect, Rockstar and the Xbox app are listed automatically (read from the launchers' own files, no account or internet), and any other program can be picked with a file dialog; each game gets a DPI stage, polling rate and button layout, switched by the tray app with the foreground window
* Dongle pairing: pair, cancel, unpair and auto-pairing
* Tray icon with battery percentage, DPI stage and polling rate menu, low battery notification
* Settings window in the style of a game client ((navigation pane with Customize / Performance / Calibration / Power / Games / Roadmap, Windows 11 style settings rows, game tiles with cover art; see DESIGN.md)): every change is written to the mouse as you make it, no apply button
* Firmware versions and serial numbers of mouse and dongle

## Settings window

Click the tray icon (or run `freenapse ui`, or start `freenapse.exe` a second
time) and the settings window opens: the same controls as Synapse's mouse
page in a Steam / Xbox-app style layout (navigation pane with the device
card, a page header, flat cards of settings rows in the Fluent 2 type ramp), drawn with
plain GDI inside the same 1 MB executable.

* **Customize**: pick a layer (Normal, Hypershift, or one per game), click a button in the list or on the mouse drawing, pick a function (default, mouse button or double-click, keyboard key with modifiers, DPI stage/cycle, media key, system sleep / power / wake, macro with its playback option, Hypershift, disabled). Mouse and keyboard functions have a Turbo toggle with a repeat rate. The MACROS section below records new macros (Stop or Esc ends the recording), renames and deletes them, and shows the macro memory left in the mouse.
* **Performance**: DPI stages (enable, count, value per stage via slider or typed, active stage, independent X and Y), Dynamic Sensitivity with the three presets and a drag-the-points curve editor, polling rate, sensor rotation.
* **Games**: your game profiles as tiles (click one to set its DPI stage, polling rate, name and program, or jump to its button layout) and the installed games found in the launchers as a grid of cover tiles (Steam's library art, Xbox logos, otherwise the program's icon on a generated tile) with launcher filter chips and a search box: click a game to create its profile. The "Add a program" tile opens a file dialog for anything else. Below, the in-game polling rate that profiles use by default.
* **Calibration**: Smart Tracking (1, 2 or 3 mm) or the asymmetric cut-off with Synapse's lift-off and landing level sliders. Surface calibration and mouse-mat presets are not offered by Synapse for this mouse, so they are not here either.
* **Power**: battery, notification threshold, idle sleep, low power mode, dongle LED.
* **Roadmap**: every Synapse feature with its status (done, partial, not yet, out of scope) and what the next releases aim for; the same content as ROADMAP.md.
* The gear opens app settings: start at login, battery refresh, re-import from the mouse, re-apply everything, onboard profile selector, dongle pairing (pair, cancel, unpair, auto-pairing), device info.

Changes go to the config file and to the mouse immediately. If the mouse is
asleep the window keeps the change pending and writes it when the mouse wakes
up (the status line under the content tells you). Esc closes the window; the tray icon stays.

See [FEATURES.md](FEATURES.md) for the full Synapse feature map and the USB protocol reference, and [ROADMAP.md](ROADMAP.md) for what is done, what is partial and what the next releases aim for.

## Quick start

```
freenapse.exe               # double-click: tray icon (battery, DPI stage, polling rate, per-game profiles, macros)
freenapse ui                # settings window (or click the tray icon)
freenapse status            # see what the mouse currently has stored
freenapse config            # creates the config file from the mouse's current settings; edit it
freenapse apply             # write the config to the mouse
freenapse autostart on      # start the tray at login
```

One file, `freenapse.exe`: started without arguments it is the tray app, with a
command it is the CLI. The config lives in `%APPDATA%\freenapse\config.toml`;
put a `config.toml` next to the executable instead and that one is used
(portable mode, e.g. on a USB stick). The first config file is built from what
the mouse already has stored (the settings Synapse left in its onboard memory),
so nothing changes until you edit it. `freenapse import` rebuilds it from the
mouse at any time.

Because the same executable is a windowed program, in cmd and PowerShell the
prompt can come back before a command's output; the output still appears.

**Keep moving the mouse while running commands.** The wireless mouse stops
answering the dongle when it has been idle for a while. The tray app knows this
and retries when you come back.

Other commands:

```
freenapse dpi 1600                      live DPI
freenapse stages 2 400 800 1600 3200    DPI stages, 2nd active
freenapse polling 4000
freenapse idle 300                      sleep after 5 minutes
freenapse low-battery 15
freenapse lift smart 1                  Smart Tracking, 1 mm
freenapse lift asym 14 12               asymmetric cut-off: lift-off level 14, landing level 12
freenapse dynamic natural               Dynamic Sensitivity preset
freenapse rotation -5                   sensor angle correction
freenapse dongle-led 3                  dongle LED off
freenapse buttons                       show the onboard button mapping
freenapse macros                        macros stored in the mouse; macro-read / macro-write / macro-delete <id>
freenapse pair-status                   paired devices; pair / pair-cancel / unpair --yes
freenapse games                         installed games found in the launchers
freenapse raw 07 80 02                  send a raw command (class, id, size, args in hex); raw -d ... goes to the dongle
```

A macro in the config file looks like this; `freenapse apply` writes the
button binding to the mouse (the Viper V3 Pro has no macro memory, so the
button sends F13..F24 and the tray app plays the macro when it sees that key):

```toml
[buttons]
back = "macro:1"            # macro:1:3 plays it 3 times; macro-held:1, macro-toggle:1, macro-seq:1

[[macros]]
id = 1
name = "Reload and jump"
events = ["press:r", "delay:80", "press:space"]

[[games]]                   # per-game profile, applied by the tray app
name = "Counter-Strike 2"
exe = "cs2.exe"             # or dir = '<install folder>': any program started from inside it matches
dpi_stage = 3
polling = 8000
[games.buttons]
forward = "macro-held:1"
```

## Replacing Synapse

1. Run `freenapse status` once with Synapse still installed to confirm the mouse answers.
2. `freenapse apply`, then `freenapse autostart on` if you want the tray.
3. Uninstall Razer Synapse (Settings > Apps). freenapse does not need any Razer
   software or driver; it talks to the standard Windows HID stack.

Tested here: after Synapse 4's silent uninstaller ran (no Razer process,
service, startup entry or uninstall entry left), `freenapse status`, `apply`,
polling switching, button reads and the tray all kept working unchanged. The
uninstaller leaves a few small folders behind (`C:\Program Files\Razer`,
`C:\Program Files (x86)\Razer`, `C:\ProgramData\Razer`,
`C:\Windows\Installer\Razer`) that can be deleted by hand; the signed Razer
HID drivers stay in the driver store and are harmless.

Do not run both for long: while Synapse is running in the background it
rewrites the settings it thinks it owns within about 15 seconds, so anything
freenapse sets appears to "not stick". Quit or uninstall Synapse once you are
happy.

## Building

Needs only [rustup](https://rustup.rs). No Visual Studio required: the project
pins the `stable-x86_64-pc-windows-gnu` toolchain (`rust-toolchain.toml`),
which ships its own linker.

```
cargo build --release
target\release\freenapse.exe      # the whole program; copy it anywhere
```

`build.rs` embeds `assets/freenapse.ico` (drawn by `assets/make-icon.ps1`)
as the executable's icon without needing `windres`.

If you do have the MSVC toolchain, delete `rust-toolchain.toml` and it
builds the same way. `cargo test` runs the protocol and config unit tests
(no hardware needed).

## How it talks to the mouse

Razer devices accept 90-byte feature reports on their first HID interface. The
report carries a transaction id (0x1f for the mouse, 0xff for the dongle), a
command class and id, a data size, 80 argument bytes and an XOR checksum. The
command layouts come from the GPL OpenRazer driver, the razerqdhid protocol
notes, and the behaviour of Synapse 4 with this mouse as observed on one
machine. `src/protocol.rs` has the exact bytes; NOTICE.md says what was and
was not used.

The code is plain Win32: SetupAPI enumeration, `HidD_SetFeature` /
`HidD_GetFeature`, `Shell_NotifyIcon` for the tray. No hidapi, no GUI framework.

## Not replicated

Chroma lighting (the Viper V3 Pro has none), cloud profiles, Sensitivity
Matcher, software-side bindings (launch a program, type text), firmware
updates. The onboard profile selector (config, CLI, gear page) is there, but
the Viper V3 Pro holds a single profile and refuses others, and Synapse's
engine for this mouse has no surface calibration, so there is none here.
See FEATURES.md, and ROADMAP.md for what the next releases aim to add.

## Licence

MIT, see [LICENSE](LICENSE). [NOTICE.md](NOTICE.md) lists the trademarks,
where the protocol knowledge comes from and the third-party crates in the
executable. [CONTRIBUTING.md](CONTRIBUTING.md) has the provenance rules every
change must follow.
