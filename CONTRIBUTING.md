# Contributing

Thanks for looking. freenapse is small and opinionated, and it writes to the
onboard memory of a mouse, so the rules below are mostly about keeping it safe
and keeping it clean.

## Before you start

* It is developed and tested on one mouse: the Viper V3 Pro with the
  HyperPolling dongle. If you have other Razer hardware, say which, and mark
  anything you could not verify as unverified in `FEATURES.md`.
* Build with `cargo build --release`; `cargo test` runs the protocol and config
  unit tests without hardware. See the Building section in `README.md`.
* Stop the tray app before building: Windows will not overwrite a running
  executable.

## Provenance rules

These are not negotiable, because they are what keeps the project
distributable.

1. **No Razer material.** Do not add code, strings, images, fonts, icons, JSON,
   firmware or any other file taken from Razer Synapse, the Razer drivers or
   the device firmware, in any form.
2. **No decompiling.** Protocol knowledge comes from what the device answers,
   from Synapse's observable behaviour as a user of it, and from public
   documentation. Nothing from disassembling or decompiling Razer binaries.
3. **No copied GPL code.** OpenRazer is a fine reference for command numbers
   and layouts; its code is GPL-2.0 and freenapse is MIT, so do not copy or
   translate OpenRazer functions. Write the Rust yourself from the facts.
4. **Say where it came from.** Every new command in `FEATURES.md` gets its
   source and its verification status.
5. **No Razer trade dress.** Keep the accent colour, the logo and the wording
   as they are in `DESIGN.md`. The project must not look like a Razer product.

## Licence of contributions

By opening a pull request you agree that your contribution is licensed under
the MIT License of this repository, with no other terms. If you are not able
to grant that, please do not submit.

## Adding a mouse

freenapse only writes to mice in the device table of `src/device.rs` that an
owner has verified. Any other Razer mouse opens read-only, so the first step
costs nothing:

1. Run `freenapse probe` with the mouse awake (keep moving it) and paste the
   whole output, which is already a code block, into a new issue. It shows the
   product id, which transaction id answered, firmware and serial, battery,
   DPI and stages, which of the two polling commands answered, the profile and
   macro-memory answers and the sensor registers, each with its raw status and
   argument bytes. Say which mouse and dongle it is and whether it was wired.
2. From that, a `DeviceSpec` row is written: product id, name, wireless flag,
   transaction id, polling command and rates, DPI maximum (OpenRazer's device
   list is a fine source for the maximum and the ids; take facts from it, not
   code), whether the Focus Pro tracking page of `src/sensor.rs` applies, and
   whether dongle pairing applies. New rows start with `verified false`.
3. With the row in place, the owner sets `[device] allow_unverified = true`,
   runs `freenapse import`, `freenapse apply`, `freenapse status` and the
   window, and reports what worked and what the mouse refused. Every write is
   read back, so a wrong row shows up as an error, not as silent damage; a
   setting that went wrong is put back with Synapse or the previous config.
4. Once the owner confirms, the row becomes `verified true` and the mouse
   joins the table in `FEATURES.md` with their name in the verified-by column.

## Pull requests

* One change per pull request, with the `FEATURES.md` / `ROADMAP.md` rows it
  affects updated in the same change.
* Keep the zero-warning build and the passing tests.
* For anything that writes to the mouse, describe how you verified it and how
  a user gets back to the previous state if it goes wrong.
