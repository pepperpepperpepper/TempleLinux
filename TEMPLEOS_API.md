# TempleOS API compatibility layer (TempleLinux)

TempleLinux runs TempleOS programs (HolyC) **from source** using `temple-hc` + a small TempleOS-ish
API surface that maps common TempleOS calls to TempleShell rendering/input/filesystem behavior.

This document is the “what exists, where it lives, and what’s intentionally different” map.

## Where the API lives

TempleOS-ish names/constants are provided in three layers:

1) **HolyC preprocessor defines (constants/macros)** — `temple-hc` injects a built-in `#define`
   table for common TempleOS constants used by upstream sources (e.g. `MSG_*`, `CH_*`, `SC_*`,
   `WIF_*`). See `src/bin/temple_hc/03_preprocess.rs`.

2) **HolyC runtime built-ins (functions)** — many TempleOS global functions are implemented as VM
   built-ins so unmodified upstream `.HC` can call them directly. Entry points live under:
   - `src/bin/temple_hc/vm/builtins/gfx.rs` (`Gr*`, `Sprite*`, …)
   - `src/bin/temple_hc/vm/builtins/ui_input_sound.rs` (`GetChar`, `GetKey`, `ScanMsg`, `Snd`, …)
   - `src/bin/temple_hc/vm/builtins/doc_fs_settings.rs` (`Doc*`, `Win*`, `Cd`, `FileFind`, …)

3) **Rust-side helpers (for Rust apps/adapters)** — `temple_rt::templeos` provides a small set of
   TempleOS-ish constants and helpers so Rust demo apps can use familiar names. See
   `src/templeos.rs`.

Underlying all of this is `temple_rt::rt::TempleRt` (the “device layer” that talks to TempleShell
via `TEMPLE_SOCK` and draws into the shared framebuffer): `src/rt.rs`.

## API surface (what’s implemented)

This is a compatibility layer, not a full reimplementation. The guiding rule is:

- Implement the smallest subset needed by real upstream demos/apps (and keep it permissive where
  safe so ports don’t fail on unused args).

### Graphics (`Gr*` / sprites)

Commonly supported:

- `GrPlot`, `GrLine`, `GrLine3` (Z ignored), `GrRect`, `GrBorder`
- `GrCircle`, `GrCircle3` (Z ignored; optional angle args may be accepted but ignored)
- `GrEllipse` (approximated)
- `GrPaletteColorSet` (TempleOS-style `CBGR48` input)
- `Sprite3`, `Sprite3YB` (rotation/Z mostly ignored; renders sprite elements)

Backing implementation:

- Most `Gr*` built-ins map to `TempleRt` primitives like `set_pixel`, `fill_rect`,
  `draw_line_thick`, `draw_rect_outline_thick`, `draw_circle_thick`, and palette updates via
  `TempleRt::palette_color_set()`.

Intentional differences / notes:

- Palette graphics are **8bpp indices** (TempleOS-style 16-color palette by default).
- Some 3D-ish APIs exist only to keep upstream sources compiling; extra dimensions may be ignored.
- `GrFloodFill` is currently a permissive **no-op** (upstream demos that need it should drive a real
  implementation).

### Input + messages (`GetChar`, `GetMsg`, `MSG_*`)

Commonly supported:

- `GetChar`, `GetKey` (minimal blocking input with optional echo)
- `ScanMsg(mask)` / `GetMsg(task, msg_ptr)` message queue APIs
- Menu-related message paths (`MSG_CMD`) via `MenuPush`/`MenuPop` overlays

Backing implementation:

- TempleShell emits input events over IPC (`temple_rt::protocol`).
- The HolyC runtime converts those into TempleOS-ish `MSG_*` messages and scan-code flags (subset).
  Key mapping logic lives in `src/bin/temple_hc/vm/09_ui.rs` (search for `map_key_event_to_msg`).

Intentional differences / notes:

- The message/scan-code mapping is a **subset** and can be extended when a real upstream app needs
  more keys or flags.
- HolyC also exposes internal `KEY_*` constants (TempleLinux protocol codes) for convenience; those
  are not TempleOS scancodes.

### Filesystem (`Cd`, `FileFind`, `::/` path specs)

Commonly supported:

- `Cd(dirname=NULL, make_dirs=FALSE)` and relative file resolution
- `FileRead`, `FileWrite`, `FileFind`, `DirMk` (subset)

Backing implementation:

- TempleOS-style `::/…` specs resolve to the vendored TempleOS tree (`TEMPLEOS_ROOT`).
- Writes are confined to the writable Temple root (`TEMPLE_ROOT`) so vendored sources remain
  read-only.

Intentional differences / notes:

- TempleOS `#exe{...}` is not executed; `__DIR__`/`__FILE__` are handled as lexer built-ins instead
  so common `Cd(__DIR__)` patterns still work.

### Sound (`Snd`, `Beep`)

Commonly supported:

- `Snd(ona)` + `Mute(val)` and `Beep(...)` (TempleOS-ish, subset)

Backing implementation:

- HolyC runtime uses `TempleRt::snd()` / `TempleRt::mute()` which send IPC to TempleShell.

Intentional differences / notes:

- Audio is best-effort: headless runs must not crash if no output device exists.

## Rust helper module: `temple_rt::templeos`

For Rust-side ports/adapters, `temple_rt::templeos` provides:

- TempleOS-ish constants (`CH_*`, `MSG_*`, `SC_*`, palette indices, …) as `pub const`.
- A minimal `CDC { color, thick }` to keep the “device context” pattern.
- Small `Gr*` wrappers that call into `TempleRt` primitives.

This module is intentionally tiny; most “real” compatibility work happens in the HolyC runtime.
