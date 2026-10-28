# Notices

## Licence

freenapse is released under the MIT License; see [LICENSE](LICENSE). That
covers the source code, the documentation and the project's own artwork
(`assets/logo.svg`, the wordmarks, the icon and the script that draws it).

## Trademarks

Razer, Razer Synapse, Viper, HyperSpeed, HyperPolling and Focus Pro are
trademarks or registered trademarks of Razer Inc. freenapse is an independent
project. It is not affiliated with, endorsed by, sponsored by or supported by
Razer Inc. Those names appear in this repository only to identify the hardware
and software the project interoperates with.

The name "freenapse" and the open-ring logo belong to this project. They are
not derived from any Razer artwork.

## Where the protocol knowledge comes from

freenapse talks to the mouse with the vendor's HID feature-report protocol.
That knowledge was gathered for interoperability only, from:

* the mouse and dongle themselves: their answers to commands, observed on one
  Viper V3 Pro with its HyperPolling dongle;
* the behaviour of Razer Synapse 4 on that machine as a user of it, observed
  through its logs and the traffic it exchanges with the mouse;
* [OpenRazer](https://github.com/openrazer/openrazer), the GPL-2.0 Linux
  driver, read as documentation of command numbers and argument layouts;
* the razerqdhid protocol notes.

What the repository does **not** contain:

* no code, text, images, fonts, sounds or other files from Razer Synapse, the
  Razer drivers or the device firmware;
* nothing obtained by decompiling or disassembling a Razer executable, driver
  or firmware image;
* no code copied or translated from OpenRazer or any other GPL project. The
  Rust implementation in `src/` was written from scratch. Command classes,
  ids, sizes, byte layouts and status codes are facts about the wire format
  and are not anyone's copyrightable expression.

Every documented command in `FEATURES.md` says where it comes from and
whether it was verified on hardware.

## Third-party components

The release executable statically links the following Rust crates. Each is
used under the terms its authors chose; the full texts are in each crate's
repository and in the local cargo registry.

| Crate | Licence |
|---|---|
| serde, serde_core, serde_derive, serde_spanned | MIT OR Apache-2.0 |
| toml, toml_datetime, toml_parser, toml_writer | MIT OR Apache-2.0 |
| winnow | MIT |
| proc-macro2, quote, syn | MIT OR Apache-2.0 |
| unicode-ident | (MIT OR Apache-2.0) AND Unicode-3.0 |
| windows-sys, windows-targets, windows_x86_64_gnu | MIT OR Apache-2.0 |

`cargo tree --format "{p} {l}"` prints the current list.

freenapse links only against Windows system libraries (kernel32, user32,
gdi32, gdiplus, setupapi, hid, shell32, advapi32, dwmapi, comctl32, comdlg32,
ole32) through the import libraries bundled with `windows-sys`. It bundles no
third-party DLLs.

## Fonts

The settings window and the SVG wordmarks use Segoe UI Variable / Segoe UI,
which ship with Windows. No font file is distributed with freenapse; on a
system without those fonts the SVGs fall back to Inter or the system
sans-serif.
