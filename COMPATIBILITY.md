# TempleLinux compatibility target + smoke suite

TempleLinux is **not** the TempleOS kernel. The compatibility target is:

- Run a representative set of **unmodified** HolyC programs from the vendored TempleOS source tree (`third_party/TempleOS/`) inside **TempleShell**.
- Match the **TempleOS look/feel** (font, palette, UI conventions) for the “Temple workspace”, while Linux apps run normally on Linux.

This document defines what “runs TempleOS apps” means for this repo, and how we keep it from regressing.

For a map of the implemented TempleOS-ish APIs/constants (and intentional differences), see `TEMPLEOS_API.md`.

## Terminology / paths

- `TEMPLEOS_ROOT`: path to the vendored TempleOS tree (typically `third_party/TempleOS/`).
- `TEMPLE_ROOT`: writable “Temple drive” root used for user data and TempleLinux-specific apps (typically `~/.templelinux/root` when running TempleShell).
- TempleOS-style `::/…` specs resolve into `TEMPLEOS_ROOT`.

## Compatibility definition (“done enough to claim it works”)

TempleLinux is considered to “run TempleOS apps” when all of these hold:

1) **Protocol-level smoke**: a representative set of upstream sources compile + run end-to-end against a fake `TEMPLE_SOCK` and exit deterministically.
2) **GUI-level smoke**: TempleShell can run headlessly (Xvfb) and produce deterministic 640×480 screenshots for representative UI surfaces (shell, docs, editor, menus, multi-window, and a few upstream demos/games/apps).
3) **Manual sanity** (non-automated): the same representative set is usable interactively on a real display (mouse+keyboard behave, ESC exits, etc.).

## Representative smoke list (current)

This list is intentionally biased toward “full TempleOS UI experience” rather than only shell output.

### Docs / UI surfaces (TempleShell)

- Shell initial frame (`templeshell --test-dump-initial-png …`)
- File browser (`files /Home/Demo`)
- DolDoc viewer:
  - `help DemoIndex`
  - `help HelpIndex` (sprite-heavy)
  - `help DolDocOverview` (escaped `$` + literal commands)
  - `help Job` (table/layout heavy)
- Pull-down menu demo (`tapp hc ::/Demo/PullDownMenu.HC`)
- Editor (`tapp edit /Home/EditorDemo.txt`)
- LinuxBridge (`tapp linuxbridge`)
- Multi-window layout (Paint + Demo)
- Wallpaper desktop layer (WallPaperFish as background + normal window on top)

### Upstream demos (HolyC, unmodified unless noted)

- `::/Demo/Print.HC`
- `::/Demo/NullCase.HC`
- `::/Demo/SubSwitch.HC`
- `::/Demo/MsgLoop.HC`
- `::/Demo/PullDownMenu.HC`
- `::/Demo/ExtChars.HC` (CP437/glyph-patching)
- `::/Demo/Graphics/NetOfDots.HC`
- `::/Demo/Graphics/MouseDemo.HC`
- `::/Demo/Graphics/Lines.HC`
- `::/Demo/Graphics/SpritePlot.HC`
- UI/controls-heavy:
  - `::/Demo/Graphics/Slider.HC`
  - `::/Demo/Graphics/ScrollBars.HC`
  - `::/Demo/Graphics/Grid.HC`
  - `::/Demo/Graphics/Palette.HC`
  - `::/Demo/Graphics/WallPaperCtrl.HC` (via TempleLinux wrapper `tapp wallpaperctrl`)
  - `::/Demo/Graphics/WallPaperFish.HC` (via TempleLinux wrapper `tapp wallpaperfish`)

### Upstream games/apps (HolyC, unmodified unless noted)

- Games:
  - `::/Demo/Games/TicTacToe.HC`
  - `::/Demo/Games/CharDemo.HC`
- Apps:
  - `::/Apps/TimeClock/*` (unmodified core; launched via TempleLinux wrapper `tapp timeclock`)
  - `::/Apps/Logic/Run.HC` (`tapp logic`)
  - `::/Apps/KeepAway/Run.HC` (`tapp keepaway`)

## How to run the smoke suite

### Fast (no GUI): protocol-level tests

Run:

```bash
cargo test -q
```

These tests run unmodified upstream `.HC` programs over a fake `TEMPLE_SOCK` and assert text/present behavior.

### GUI-level goldens (X11/Xvfb)

Run (Linux only):

```bash
TEMPLE_GUI_TESTS=1 cargo test -q gui_smoke
```

Notes:

- Requires `xvfb-run` in `PATH`.
- Headless GL can be finicky on some hosts; the code/tests prefer Mesa software rendering.

### Screenshot gallery (capture + upload)

If `wtf-upload` is in `PATH`, run:

```bash
packaging/bin/templelinux-publish-screenshots
```

Environment variables:

- `TEMPLE_UPLOAD_PREFIX=templelinux/screenshots/...` to control upload prefix.
- `TEMPLE_SCREENSHOT_DIR=/path` to keep outputs instead of using a temp dir.
- `TEMPLE_XVFB_ARGS=...` to customize Xvfb screen settings.

The script uploads PNGs plus an `index.html` gallery.
