# TempleLinux

TempleLinux is a TempleOS-inspired environment for Linux: a full-screen “Temple” UI that aims for TempleOS-like visuals (palette, font, DolDoc, sprites) while running on a normal Linux desktop.

This repo vendors upstream TempleOS sources under `third_party/TempleOS/` and focuses on running representative, unmodified HolyC programs inside `templeshell`.

If you cloned without submodules, run:

```bash
git submodule update --init --recursive
```

## Overview (from `analysis.md`)

TempleLinux is a Linux-native software stack that tries to recreate the **TempleOS “feel”** without running the TempleOS kernel:

- **`templeshell`** is a graphical host shell that renders into a fixed internal framebuffer (currently **640×480**) and scales it with **nearest-neighbor** + **4:3 letterboxing** for pixel fidelity.
- **`temple-hc`** runs TempleOS programs from their **HolyC source**, backed by a practical TempleOS API compatibility layer (graphics/input/files/docs/etc).
- Linux remains “real” underneath: normal Linux apps are launched normally (e.g. via `xdg-open`) and can live on a separate workspace in the dedicated session mode.

What it is *not*:

- Not the TempleOS kernel or a VM/emulator image.
- Not binary compatibility for TempleOS ISOs/binaries.

More detail: `analysis.md` and `research.md`.

## Screenshots

<p>
  <img src="docs/screenshots/2026-02-15/personalmenu.png" width="320" alt="PersonalMenu icons">
  <img src="docs/screenshots/2026-02-15/doldoc-helpindex.png" width="320" alt="DolDoc HelpIndex">
</p>

<p>
  <img src="docs/screenshots/2026-02-15/multiwindow.png" width="320" alt="Multi-window (Paint + Demo)">
  <img src="docs/screenshots/2026-02-15/wallpaperfish.png" width="320" alt="WallpaperFish background">
</p>

More screenshots: `docs/screenshots/2026-02-15/`

## Smoke tests

- Protocol-level: `cargo test -q`
- GUI goldens (headless/Xvfb): `TEMPLE_GUI_TESTS=1 cargo test -q --test gui_smoke`

See `COMPATIBILITY.md` for the current compatibility target + smoke suite definition.
