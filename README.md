# TempleLinux

TempleLinux is a TempleOS-inspired environment for Linux: a full-screen “Temple” UI that aims for TempleOS-like visuals (palette, font, DolDoc, sprites) while running on a normal Linux desktop.

This repo vendors upstream TempleOS sources under `third_party/TempleOS/` and focuses on running representative, unmodified HolyC programs inside `templeshell`.

If you cloned without submodules, run:

```bash
git submodule update --init --recursive
```

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
