# TempleLinux (TempleOS apps + aesthetics on Linux) — Detailed Plan

## 0) Executive summary

Goal: **make a TempleOS-like environment on Linux that matches TempleOS visually and can run original TempleOS apps**, while still being able to run **all normal Linux apps** (Chrome, editors, terminals, etc.).

TempleLinux’s approach:

- Linux stays “real” underneath: drivers, networking, package management, normal apps.
- The vendored TempleOS tree (`third_party/TempleOS/`) is treated as the **source-of-truth** for apps/docs/assets.
- TempleOS apps are run **from their source** (HolyC) inside our environment via a HolyC-compatible runtime and a TempleOS API compatibility layer (without running TempleOS as a full OS).
- A Wayland session (e.g. `sway`) provides a clean “Back to Temple” flow:
  - workspace 1: TempleShell full-screen (TempleOS look, pixel-perfect)
  - workspace 2: normal Linux apps
- TempleShell is the “Temple” environment:
  - runs TempleOS apps,
  - provides a TempleOS-like shell/UI,
  - provides workspace switching helpers,
  - optionally provides bridges (launcher, clipboard/file bridge UI, etc.).

Visual fidelity requirement:

- The “Temple look” must use **TempleOS visual assets** (fonts/palette/UI bits) and match TempleOS ergonomics as closely as practical.

---

## 1) Goals / non-goals

### Goals

- **Run original TempleOS apps** (source-level HolyC programs from the TempleOS tree) inside TempleLinux.
- **TempleOS feel, Linux reality**: TempleOS workflow/visuals as primary, with Linux available for “modern stuff”.
- **Reuse TempleOS as much as possible**: treat `third_party/TempleOS` as the canonical apps/docs/assets tree; prefer “run it” over re-implementing apps in Rust.
- **Use TempleOS assets**: font(s), palette(s), and other visual resources come from the TempleOS source tree (not “close enough” substitutes).
- **Pixel-perfect output**: nearest-neighbor scaling, integer scale where possible, and no compositor filtering.
- **Run Linux apps normally**: Chrome, editors, terminals, etc. run as native Linux apps (Wayland/XWayland), typically on a separate workspace.
- **Integration bridges** (opt-in): clipboard, shared folder, and “open URL / open file / run command” bridging between TempleOS and Linux.

### Non-goals (at least initially)

- Running the TempleOS kernel/boot process (we are not running a guest OS).
- Running TempleOS binaries/ISOs without source (binary compatibility is not the strategy).
- Perfect 1:1 behavior for every low-level API on day one (we’ll grow compatibility based on real apps).
- Theming Linux apps to look like TempleOS (possible later, but the primary “Temple look” is the Temple workspace).

---

## 2) Terms

- **Compositor**: Wayland compositor (`sway`, `weston`, `labwc`, etc.) that manages all windows.
- **TempleOS tree**: the vendored upstream sources under `third_party/TempleOS/`.
- **TempleOS app**: a HolyC program from the TempleOS tree (e.g. under `Apps/`, `Demo/`, `Adam/`).
- **TempleOS API layer**: a compatibility surface that implements the functions/types TempleOS apps expect (graphics, docs, windowing, input, files).
- **HolyC runtime**: the interpreter/bytecode engine/compiler used to execute HolyC on Linux.
- **Linux workspace**: workspace where normal Linux apps run.
- **TempleShell**: the full-screen Wayland client that renders the TempleOS-like UI and hosts the HolyC runtime + TempleOS API layer.
- **temple-rt**: the low-level runtime + IPC protocol used by external Temple apps (Rust/C); also useful as the “device layer” under the TempleOS API layer.

---

## 3) System architecture

### 3.1 Session layout

1) Linux boots into a minimal Wayland session.
2) The compositor starts **TempleShell** and puts it **full-screen** on workspace 1.
3) TempleShell loads/runs TempleOS apps (HolyC) and renders the result pixel-perfect.
4) Linux apps run normally on workspace 2 (or beyond).
5) TempleShell provides:
   - a “Back to Temple” command/hotkey helper,
   - a Linux app launcher that doesn’t break the TempleOS vibe,
   - bridges (clipboard/shared folder) and diagnostics.

When launching Linux apps:
   - They are started as normal processes.
   - They show up as normal windows in the compositor.
   - Optionally, TempleShell requests the compositor to place them on a different workspace and switch focus.

### 3.2 Recommended compositor (pragmatic choice)

Pick one compositor to target first, then keep the design compositor-agnostic:

- **Recommended**: `sway` (easy config + robust IPC).
- Alternatives: `weston` (reference compositor), `labwc` (Openbox-like on wlroots), `river`/`Hyprland` (if you already use them).

Target behavior:

- Workspace 1: TempleShell full-screen.
- Workspace 2: Linux apps.
- Optional workspace 3: extra Linux apps / scratch.
- A single hotkey (“Back to Temple”) switches to workspace 1.

### 3.3 TempleOS sources + assets (local reference)

TempleOS source is vendored locally to enable:

- extracting fonts/palette/constants for the host layer,
- referencing original APIs/naming for compatibility shims,
- building future tooling around the original tree (docs indexing, port notes, etc.).

Local path:
- `third_party/TempleOS/` (cloned from the CIA Foundation repo).
Upstream:
- `https://github.com/cia-foundation/TempleOS.git`

Vendoring policy:
- Treat `third_party/TempleOS/` as an **upstream mirror** (keep it clean so updating is easy).
- Put TempleLinux-specific HolyC apps/scripts in a separate tree (e.g. under the host share drive) and avoid forking TempleOS unless absolutely necessary.

### 3.4 “Reuse first” rule (use TempleOS as much as possible)

Hard requirement recap:
- The system must **run original TempleOS software**.
- The system must **match TempleOS visually** and **use TempleOS visual assets**.

Practical strategy:

1) **Run TempleOS apps from the TempleOS tree** (HolyC) inside TempleLinux.
   - Treat `third_party/TempleOS` as the canonical app/docs corpus and make it runnable.

2) **Prefer “use TempleOS code/assets” over re-implementation** whenever we add host-side features:
   - UI assets: font, palette, line-drawing/border chars, UI conventions.
   - Docs: prefer TempleOS `.DD`/DolDoc content as the source of truth.
   - Tools/apps: prefer to run the original HolyC sources, not rewrite them in Rust.

3) Use host-side code only for what TempleOS intentionally doesn’t do well:
   - modern browser (Chrome/Firefox), editors, networking-heavy apps,
   - integration glue (workspace switching, bridges),
   - optional overlays that do *not* alter the TempleOS look.

---

## 4) Display model (TempleOS pixel fidelity)

### 4.1 Resolution & scaling

- **Target internal resolution**: TempleOS defaults to **640×480**, 16-color (`GR_WIDTH × GR_HEIGHT` in TempleOS sources).
  - Default TempleShell mode should be **640×480** to match TempleOS.
  - Optional: support additional internal sizes later (e.g. 800×600) for “bigger Temple” without losing crispness.
  - Current codebase note: early milestones used an internal 800×600 buffer; Milestone 30 is the explicit “switch to TempleOS assets + 640×480 default” task.
- **Output resolution**: compositor-provided window size (ideally the monitor’s native mode).
- **Scaling**: nearest-neighbor only. Prefer integer scale; letterbox when needed.

Scaling policy:

- Default: **integer scaling + letterboxing**
  - Compute `scale = min(floor(out_w/internal_w), floor(out_h/internal_h))`, clamp to at least 1.
  - Render the scaled image centered.
  - Fill borders with a solid color (or a simple Temple-style pattern later).
- Optional: **stretch-to-fill**
  - Scale independently in X/Y (still nearest).
  - Accepts non-integer scaling artifacts but avoids letterboxing.

### 4.2 Avoid compositor filtering

Do not rely on the compositor to scale the surface (it may apply linear filtering).
Instead:

- Render a full-resolution frame (native output size) every present.
- Perform the nearest-neighbor “upscale” inside TempleShell (GPU is easiest/fastest).

### 4.3 Input coordinate mapping

All UI logic uses internal coordinates (`internal_w × internal_h`).

- Keyboard: direct.
- Mouse/touch:
  - Map output pixel coords → internal coords:
    - subtract letterbox offset,
    - divide by integer scale (or handle stretch-to-fill factors),
    - clamp to `[0..internal_w-1]×[0..internal_h-1]`.
  - When pointer is outside the scaled region, either clamp or treat as “no hit”.

### 4.4 Present loop

Target a simple loop:

- **Event-driven** redraws (only redraw on input/timers) for low CPU usage.
- A “forced vsync” mode can be added later if needed for animations.

---

## 5) TempleShell UI/UX design

### 5.1 UI primitives (MVP)

- Fixed palette: **TempleOS std palette (16 colors)** from the TempleOS sources.
  - Source of truth: `third_party/TempleOS/Adam/Gr/GrPalette.HC` (`gr_palette_std`, `gr_palette_gray`, etc.).
- Bitmap font: **TempleOS 8×8 font** from the TempleOS sources.
  - Source of truth: `third_party/TempleOS/Kernel/FontStd.HC` (`sys_font_std`).
- Box/border characters (optional but strongly TempleOS-ish):
  - Source of truth: `third_party/TempleOS/Kernel/KMain.HC` (`text.border_chars[...]`) and related kernel text constants.
- Sprites/bitmaps/icons (optional, later):
  - Source of truth: TempleOS sprite code/bitmaps under `third_party/TempleOS/Adam/Gr/` (e.g. `Sprite*.HC`).
- 2D primitives:
  - fill rect
  - draw rect/line
  - draw glyph / draw string
  - blit 1bpp/8bpp bitmaps

### 5.2 Shell features (MVP)

TempleShell’s role is to be the **TempleOS-like environment** on Linux:

Minimum useful feature set:

- **Run TempleOS apps** (HolyC) from `third_party/TempleOS/` and show clear errors if an app fails to load/compile/run.
- **App launcher** that can browse/search the TempleOS tree (Apps/Demo/Doc) and run things without memorizing paths.
- **Workspace switching helpers** (“Back to Temple”, “Go to Linux apps”).
- **Linux app launching** without breaking the vibe:
  - `run <linux-command...>`
  - `browse <url>` (via `xdg-open`)
  - `open <path>` (via `xdg-open`)
- A simple **command prompt** with history/line editing.
- A small **status/notifications log** (app crashes, bridge errors, app launches).

### 5.3 “Temple feel” checklist

- No anti-aliasing, no subpixel text.
- Nearest-neighbor scaling only.
- Crisp pixel grid; integer scaling preferred.
- Instant feedback (fast input-to-draw).
- Minimal chrome: no modern UI widgets; keep it Temple-simple.

---

## 6) Launching Linux apps (and keeping them normal)

### 6.1 Process spawning

Linux apps should be spawned as normal host processes.

Preferred UX (most TempleOS-like):
- Launch Linux apps from inside the Temple environment via the **LinuxBridge** HolyC app (Milestone 36), so you don’t leave the Temple UI.

Fallback / power-user UX:
- TempleShell can spawn Linux apps directly as normal processes:

- Use `xdg-open` for URLs and files (respects user defaults).
- Provide “power user” commands:
  - `run -- <argv...>` to bypass parsing pitfalls.
  - `env KEY=VALUE run ...` if desired.

### 6.2 Workspace strategy (recommended)

Keep TempleOS visually “dominant”:

- Workspace 1: **TempleShell** full-screen (no gaps, no borders).
- Workspace 2: Linux apps (normal tiling/floating rules).
- Optional workspace 3: extra Linux apps / scratch.

Implementation approaches:

- **Compositor config only**: hotkeys and rules live in `sway` config; keep programs simple.
- **TempleShell-assisted**: TempleShell calls compositor IPC to:
  - switch to the Linux workspace after launching a Linux app,
  - optionally move the new window to the Linux workspace,
  - switch back to the TempleShell workspace on command.

Start with “compositor config only” to reduce complexity.

---

## 7) Temple apps execution model (HolyC compatibility)

There are two “Temple app” lanes:

### 7.1 Lane A (required): run TempleOS HolyC apps on Linux

This is the compatibility requirement: run the **original HolyC source programs** in `third_party/TempleOS/`.

Key idea:
- Provide a HolyC runtime + a TempleOS API compatibility layer that maps TempleOS calls to our renderer/input/filesystem.

Execution models (in order of bring-up difficulty):

1) **Interpreter/bytecode engine (MVP)**
   - Parse HolyC + execute in an interpreter/bytecode engine.
   - Pros: fastest iteration; easiest to support “weird” syntax incrementally.
   - Cons: slower; harder to match all semantics; needs careful sandboxing.

2) **Transpile/AOT compile**
   - Translate HolyC → C/Rust/LLVM IR and compile to a native Linux binary that links against the compatibility layer.
   - Pros: performance; easier debugging with native tooling (depending on approach).
   - Cons: much higher compiler complexity; HolyC corner cases can be painful.

3) **Hybrid**
   - Interpreter for development and fallback; AOT for frequently-used apps.

### 7.2 Lane B (optional): native “Temple-like” apps on Linux (`temple-rt`)

This lane exists for:
- writing new apps that integrate tightly with the host (bridges, launchers, tools),
- experimenting with a “Temple-ish” API,
- long-term: optional ports/re-implementations when full HolyC compatibility is not worth it.

Recommendation:
- Treat Lane A as the compatibility requirement.
- Treat Lane B as a host-side convenience layer.

---

## 8) temple-rt (compatibility runtime) design

`temple-rt` is the low-level runtime for drawing/input/events and is useful in two ways:

1) For native external apps (Rust/C) talking to TempleShell over IPC (current implementation).
2) As the “device layer” under a higher-level **TempleOS API compatibility layer** for HolyC apps.

### 8.1 Minimum API surface (v0)

Graphics:
- `SetPixel(x,y,color)`
- `FillRect(x,y,w,h,color)`
- `DrawText(x,y,"...")` (monospace only initially)
- `Blit8(x,y,w,h,src)` (8bpp paletted)

Input:
- `GetKey()` / event queue (press/release, modifiers)
- optional: mouse state

Timing:
- `SleepMs(ms)`
- `NowMs()` / ticks

Files:
- open/read/write helpers
- directory listing
- a path mapping layer so Temple apps see a “Temple-ish” filesystem view

### 8.2 Filesystem mapping strategy

Keep it boring and predictable:

- Define a “Temple root” directory, e.g. `~/.templelinux/`.
- Temple apps see `/` as that root.
- Provide host passthrough mounts later (read-only) if desired.

Example mapping:
- Temple `/` → host `~/.templelinux/`
- Temple `/Home` → host `~/.templelinux/Home`

### 8.3 IPC protocol sketch (process model)

Use a Unix domain socket for control + a shared memory region for pixels.

Handshake:
1) App connects to `TEMPLE_SOCK` (env var provided by TempleShell).
2) App requests a framebuffer.
3) TempleShell returns:
   - shm fd + size,
   - pixel format (8bpp indices),
   - palette id/version,
   - input event stream fd or multiplex on the same socket.

Present:
- App writes pixels to shm and sends `PRESENT(seq, dirty_rects?)`.
- TempleShell composites/presents the new frame (scaled up).

Input:
- TempleShell sends `KEY_DOWN`, `KEY_UP`, `TEXT_INPUT` (optional), `MOUSE_MOVE`, etc.

Versioning:
- Every message includes a protocol version.
- TempleShell rejects incompatible versions with an explicit error.

---

## 9) HolyC toolchain approach

HolyC/toolchain strategy is central, because **running TempleOS apps without running TempleOS as an OS means executing HolyC on Linux**.

Important reality:
- TempleOS sources are HolyC + asm and were designed for the TempleOS compiler/runtime.
- We should expect a long tail of compatibility work (language + API surface + build system quirks like `#exe` blocks).

Execution options:

### Path A (recommended MVP): interpreter/bytecode engine

- Parse HolyC and execute it in an interpreter/bytecode engine.
- Add features based on real programs in `third_party/TempleOS/`.

Pros:
- Fast bring-up; incremental; easiest to debug language issues.
Cons:
- Performance may be lower; precise semantics take time.

### Path B: transpile HolyC → C (fast iteration, tricky semantics)

- Build a HolyC frontend that outputs portable C (or a small C-like IR).
- Compile with `clang`/`gcc` to an ELF binary linked with `temple-rt`.
- Start with a “useful subset” of HolyC and grow based on real ports.

Pros:
- Faster bring-up, simpler backend.
Cons:
- Harder to perfectly match HolyC semantics without lots of work.

### Path C: HolyC → LLVM IR (more work, more control)

- Emit LLVM IR and rely on LLVM for codegen/optimization.

Pros:
- Better debugging/tooling, potential performance.
Cons:
- Larger upfront complexity.

### Path D: hybrid (recommended long-term)

- Interpreter/bytecode engine for broad compatibility + a fast fallback.
- Optional AOT compilation for frequently used apps or heavy demos.

Recommendation:
- Start with Path A (interpreter/bytecode engine) and use `third_party/TempleOS` as the test suite.
- Add AOT only if performance becomes a problem after compatibility is good.

---

## 10) Implementation plan (milestones + acceptance criteria)

Note on scope:
- Milestones 1–28 describe the “native TempleShell + temple-rt” track (already implemented in parts).
- To meet the hard requirement “run original TempleOS apps” (without a guest OS), the critical path is **Milestones 29+** (asset fidelity + HolyC compatibility + TempleOS API layer).

### Milestone 1: TempleShell “black screen” → scaled framebuffer
Status (2026-01-30): Completed in `templeshell` (Rust `winit` + `wgpu`), with integer scaling + letterbox via viewport/scissor and output→internal input mapping verified via a crosshair overlay.
Acceptance:
- Full-screen window on Wayland.
- Internal fixed-resolution buffer displayed with nearest-neighbor scaling.
- Integer scaling + letterbox works on resize.
- Input mapping from output → internal coordinates verified (cursor position shown or a small crosshair app).

### Milestone 2: Font + palette + text rendering
Status (2026-01-31): Completed in `templeshell` (TempleOS `sys_font_std` 8×8 font + TempleOS `gr_palette_std` palette, extracted at build time from `third_party/TempleOS/`).
Acceptance:
- Embedded bitmap font renders crisply at the internal resolution.
- Terminal-style text printing works.
- No anti-aliasing anywhere.

### Milestone 3: Command prompt + built-ins
Status (2026-01-30): Completed in `templeshell` (fixed prompt line + basic line editing, history navigation, and built-ins `help`, `clear`, `ls`, `cd`, `pwd`, `cat` operating inside the Temple root directory `~/.templelinux/`).
Acceptance:
- Command line with history.
- `help`, `clear`, `ls`, `cd`, `pwd`, `cat` working inside the Temple root directory.

### Milestone 4: Launch Linux apps
Status (2026-01-30): Completed in `templeshell` (`run`, `open`, `browse` built-ins; `open`/`browse` use `xdg-open`, and `run` spawns arbitrary commands with the working directory set to the current Temple cwd mapped under `~/.templelinux/`).
Acceptance:
- `browse <url>` opens the default browser (or configured browser).
- `open <path>` opens the file in default handler.
- `run <cmd...>` launches arbitrary commands.
- Optional: compositor config places Linux apps on a separate workspace.

### Milestone 5: temple-rt v0 + one Temple app
Status (2026-01-31): Completed (`temple_rt` runtime + IPC protocol; TempleShell hosts a Unix socket server and shares a 640×480 8bpp framebuffer via `memfd` + `SCM_RIGHTS`, forwards input, and includes a demo external Temple app `temple-demo` launchable via `tapp demo`).
Acceptance:
- A small external “Temple app” process connects via IPC.
- App can draw text/rectangles and respond to key input.
- Crash of the app does not crash TempleShell.

### Milestone 6: HolyC spike (tiny subset)
Status (2026-01-30): Completed (`temple-hc` binary: tiny HolyC-like subset interpreter with drawing built-ins calling `temple_rt`; launchable from TempleShell via `tapp hc`).
Compatibility target (tiny subset):
- Program shape: either `U0 Main()` / `U0 main()` (no params) or top-level statements.
- Lexing: whitespace, `//` line comments, `/* */` block comments, decimal integers, `"string"` literals.
- Statements: var decls (e.g. `I64 x = 1;`), assignments, `if/else`, `while`, `return`, expression statements.
- Expressions: `+ - * / %`, `== != < <= > >=`, `&& ||`, unary `- !`, parentheses, function calls.
- Values: `I64`-like integers + string literals; truthiness is `int != 0` / non-empty string.
- Built-ins: `Clear`, `SetPixel`, `FillRect`, `Text`, `Present`, `Sleep`, `NextKey`.
- Predefined constants: `SCR_W`, `SCR_H`, `KEY_ESCAPE`, `KEY_LEFT/RIGHT/UP/DOWN`.
Acceptance:
- Compile and run a tiny HolyC-like program that draws something via `temple-rt`.
- Define a compatibility target: “what subset works” (documented).

### Milestone 7: First real port
Status (2026-01-30): Completed (ported `temple-paint` Temple app; keyboard-controlled painting; launch from TempleShell via `tapp paint`; no `temple_rt` expansion needed).
Acceptance:
- Port one small Temple-style app (clock, paint, file viewer, etc.).
- Expand `temple-rt` only as needed; keep the API tight.

### Milestone 8: Session entry + compositor glue (sway)
Status (2026-01-30): Completed (added a sample Wayland session entry + sway-based launcher script under `packaging/`; TempleShell status line includes a `Super+1`/`Super+2` workspace hint).
Acceptance:
- Provide a sample Wayland `.desktop` session entry + wrapper script that starts `sway` and `templeshell`.
- Ensure XWayland is enabled for legacy apps.
- Provide a default “Back to Temple” hotkey (Super+1) and mention it in the UI.

### Milestone 9: Optional workspace switching (sway IPC)
Status (2026-01-30): Completed (added a `ws` built-in that can switch workspaces via `swaymsg`; `TEMPLE_AUTO_LINUX_WS=1` optionally auto-switches to the Linux workspace after `open`/`browse`, enabled by default in the sway launcher script).
Acceptance:
- Provide a `ws <temple|linux|num>` command that switches sway workspaces (or prints a clear error if not running under sway).
- If `TEMPLE_AUTO_LINUX_WS=1`, `open` and `browse` switch to the Linux workspace after launching.

### Milestone 10: Mouse input for Temple apps (temple-rt)
Status (2026-01-31): Completed (added mouse move + button IPC messages; TempleShell forwards cursor + button events to the active Temple app; `temple_rt` exposes `Event::MouseMove` / `Event::MouseButton`; demo apps updated to compile and `temple-demo` uses LMB to teleport the rect).
Acceptance:
- TempleShell forwards mouse movement + button events to the connected Temple app via IPC.
- `temple_rt` exposes mouse events (at least move + button) via `try_next_event()`.
- Update the demo apps to compile and either use or ignore mouse events.

### Milestone 11: Mouse-driven painting in `temple-paint`
Status (2026-01-31): Completed (`temple-paint` now follows the mouse; LMB paints and RMB erases with click/drag; MMB picks the color under the cursor; keyboard controls still work).
Acceptance:
- Cursor follows mouse position.
- LMB paints on click/drag.
- RMB erases (paints color 0) on click/drag.
- MMB picks the color under the cursor.
- Keyboard controls still work.

### Milestone 12: Keyboard + text input fidelity
Status (2026-02-02): Completed (expanded key mapping and modifier support; typed ASCII input already forwarded via `Key::Character`).
Acceptance:
- Temple apps receive key press/release events for a broad set of keys (F-keys, tab, home/end, page up/down, insert/delete, etc.).
- Temple apps can observe modifier state (Shift/Ctrl/Alt/Super) in events.
- Temple apps receive a text input event for actual typed characters (at least ASCII; UTF-8 optional).
- Key repeat behavior is predictable (either explicit repeat events or repeat handled by app).

Progress notes (2026-02-02):
- Expanded `temple_rt::protocol` key codes and TempleShell key mapping for: Tab/Home/End/PageUp/PageDown/Insert, F1–F12, and modifier keys (Shift/Ctrl/Alt/Super).
- `temple-hc` now exposes these key constants (e.g. `KEY_F1`, `KEY_TAB`, `KEY_SHIFT`) to HolyC programs.

### Milestone 13: Mouse wheel + cursor semantics
Status (2026-02-02): Completed (added wheel + enter/leave events in `temple_rt` protocol; TempleShell forwards them to apps).
Acceptance:
- Temple apps receive scroll wheel events (with a clear unit convention).
- Temple apps receive pointer enter/leave (or an explicit “pointer outside internal region” state).
- TempleShell shows a Temple-style cursor (software cursor inside the internal framebuffer if desired).

Progress notes (2026-02-02):
- Added IPC messages + `temple_rt::Event` variants: `MouseWheel`, `MouseEnter`, `MouseLeave`.
- TempleShell forwards wheel + enter/leave while an app is running (enter/leave is based on being inside the internal letterboxed region).
- Wheel unit convention: deltas are delivered in “internal pixels”; winit `LineDelta` is converted using `8px per line`, `PixelDelta` is mapped into internal pixels via the current scale.
- TempleShell now hides the host cursor and draws a small Temple-style software cursor in the internal framebuffer (disabled in test PNG dump mode to keep golden screenshots stable).

### Milestone 14: Clipboard bridge (Wayland ⇄ Temple)
Status (2026-02-02): Completed (host clipboard bridge with paste hotkey + app→host clipboard API).
Acceptance:
- Copy text from a Temple app → paste into a Linux app.
- Copy text from a Linux app → paste into TempleShell prompt and a Temple app.
- Works under Wayland (sway); gracefully degrades elsewhere.

Progress notes (2026-02-02):
- Added host clipboard integration via `arboard` and a `clip` built-in (`clip get|set|clear`) in TempleShell.
- Implemented paste hotkey: `Ctrl+V` (or `Shift+Ins`) pastes host clipboard text into the shell prompt (when no app is running) or injects it as key events into the running Temple app.
- Added `MSG_CLIPBOARD_SET` IPC message and `TempleRt::clipboard_set_text()` so Temple apps can set the host clipboard (for copy → Linux paste).
- Demo coverage:
  - `temple-demo`: press `C` to copy the status line (“coords”) to the host clipboard.
  - `temple-hc`: added a HolyC builtin `ClipPutS("text")` to set the host clipboard.

### Milestone 15: TempleShell built-ins parity (practical daily-driver set)
Status (2026-02-02): Completed (expanded built-ins toward a practical “TempleOS-ish” daily-driver set).
Acceptance:
- Add basic file ops: `cp`, `mv`, `rm`, `mkdir`, `touch` (restricted to Temple root by default).
- Add basic search/tools: `grep`, `find`, `head`, `tail`, `wc` (simple versions).
- Add `env` / `set` for controlling TempleShell env vars (within TempleShell only).
- Add `tapp list`, `tapp kill`, `tapp restart` (manage the Temple app process).

Progress notes (2026-02-02):
- File ops: implemented `cp`, `mv`, `rm` (with `-r`), `mkdir`, `touch` within the Temple root mapping.
- Basic tools: implemented `grep` (substring), `find` (recursive, optional substring), `head`, `tail`, `wc` (simple, bounded file reads).
- TempleShell-only vars: added `env` and `set` (internal override layer used by `ws` / `open` / `browse` behavior).
- Temple app management: added `tapp list`, `tapp kill`, `tapp restart` (best-effort process management + connected-state display).

### Milestone 16: Terminal scrollback + pager
Status (2026-02-02): Completed (scrollback + simple pager; makes the shell comfortable for long output).
Acceptance:
- Scrollback buffer with PgUp/PgDn (or mouse wheel if enabled).
- A simple pager (`more`/`less`) for viewing large files and command output.
- `cat` stays fast for small files; large files auto-pager (configurable).

Progress notes (2026-02-02):
- Implemented terminal scrollback (ring buffer) and viewport scrolling with `PgUp`/`PgDn`, mouse wheel (shell mode), plus `Ctrl+Home`/`Ctrl+End` for top/bottom.
- Added `more`/`less` built-ins (loads file into the terminal + starts at top).
- `cat` now auto-pagers large files by default; toggle with `TEMPLE_CAT_AUTO_PAGER=0` via `set`.
- Status line now shows `SB:-N` when viewing scrollback.

### Milestone 17: TempleDoc (DolDoc-like) docs + help system
Status (2026-02-02): Completed (TempleDoc v0: docs directory + `help <topic>` viewer with basic formatting).
Acceptance:
- A docs directory exists in the Temple root (e.g. `/Doc` mapped under `~/.templelinux/Doc`).
- `help <topic>` opens a TempleDoc page when present.
- Docs can include: colored text, headings, inline code, and links to other docs.
- Optional: “press F1 on a word” in the shell/editor to jump to docs.

Progress notes (2026-02-02):
- Created `/Doc` under the Temple root on startup (`~/.templelinux/Doc` by default).
- `help <topic>` now checks for `/Doc/<topic>.TD` / `.td` / `.txt` and displays it when present.
- Added a tiny TempleDoc renderer:
  - Headings: lines starting with `#` are colored.
  - Inline code: text wrapped in backticks uses a different color.
  - Links: `[[topic]]` is colored (non-interactive for now).

### Milestone 18: Settings + persistence
Status (2026-02-02): Completed (history + vars persistence + AutoStart script).
Acceptance:
- Persistent command history saved under the Temple root.
- Config file for keybindings, default palette/theme, workspace settings, and defaults for `open`/`browse`.
- Optional startup script (TempleOS-ish “autoexec”): e.g. `/Cfg/AutoStart.tl` executed on launch.

Progress notes (2026-02-02):
- Added `Cfg/` under the Temple root and persisted:
  - `Cfg/History.txt` (command history; loaded on start, saved on every command).
  - `Cfg/Vars.txt` (TempleShell vars from `set`; loaded on start, saved on changes).
- Added optional startup script: `Cfg/AutoStart.tl` (runs on launch, best-effort, skipped during test PNG dump mode to keep GUI golden tests deterministic).

### Milestone 19: In-Temple text editor (Adam-like)
Status (2026-02-02): Completed (fullscreen editor Temple app with selection/search/help overlay).
Acceptance:
- `edit <path>` opens a file in an internal editor (Temple app).
- Supports: cursor movement, insert/delete, save, open, search, and basic selection.
- Displays line numbers and a status bar (filename, row/col, modified indicator).
- Integrates with `help` / TempleDoc links for a TempleOS-like dev loop.

Progress notes (2026-02-02):
- Added `temple-edit` Temple app (fullscreen editor) and wired `edit <path>` + `tapp edit <file>` in TempleShell.
- Implemented: open/create, cursor movement, insert/delete, newline split/merge, PgUp/PgDn scrolling, save (`Ctrl+S`), quit (`Ctrl+Q`), line numbers, status bar, and modified indicator (`*`).
- Implemented selection (Shift+arrows/Home/End/PgUp/PgDn) with highlight and selection-aware editing (typing replaces selection; Backspace/Delete remove selection).
- Implemented find (`Ctrl+F` prompt, Enter to find, `F3` find next with wrap); match is selected/highlighted.
- Implemented clipboard workflow: `Ctrl+C` copies selection to the host clipboard; `Ctrl+X` cuts (copy+delete); `Ctrl+A` selects all.
- Implemented docs integration: `F1` opens a help overlay for the word under cursor (or current selection), loading from `$TEMPLE_ROOT/Doc` and rendering basic TempleDoc styling (headings, inline code, `[[links]]`).

### Milestone 20: File browser + app launcher UI
Status (2026-02-02): Completed (fullscreen file browser mode in TempleShell; clickable/navigable files/apps tabs).
Acceptance:
- A Temple app that can browse directories, open files, and launch Temple apps.
- Launching a Temple app is discoverable without typing (`tapp` remains available).
- Directory browsing respects the Temple root mapping and shows clear errors for host paths.

Progress notes (2026-02-02):
- Added a fullscreen file browser UI mode inside TempleShell:
  - `files` / `fm` enters the browser (uses the Temple root mapping).
  - `Tab` switches between **Files** and **Apps** (discoverable launcher list).
  - Files: arrows/PgUp/PgDn/Home/End navigate, Enter opens directories or opens files in `temple-edit`, Backspace goes up, Esc exits browser.
  - Apps: Enter launches common Temple apps (demo/HolyC/paint/editor) without typing.
  - Mouse support: wheel scrolls the selection/list; click selects; double-click opens/launches.

### Milestone 21: Multi-window / multi-app Temple environment
Status (2026-02-03): Completed (TempleShell now supports multiple simultaneously connected Temple apps in separate internal windows, closer to the TempleOS “desktop feel”).
Acceptance:
- Run at least two Temple apps simultaneously in separate internal windows.
- Mouse focus and keyboard focus go to the active internal window.
- Basic window operations: move, focus switch, close (and maybe tile/cascade).
- App crashes only close the crashed app’s window (TempleShell stays alive).

Progress notes (2026-02-03):
- IPC now supports **multiple app connections** (each connection gets a unique app id; extra connections are no longer rejected).
- Added a minimal **internal window manager**:
  - Each app is displayed inside a framed window (title bar + close box) over the TempleShell background.
  - Focus: click a window to focus; `Alt+Tab` cycles focus.
  - Move: drag the title bar.
  - Close: click the close box or press `Ctrl+W` (sends `SHUTDOWN` to the app; TempleShell stays alive).
  - Mouse events route to the window under the cursor (client area) with coordinate scaling; keyboard events route to the focused window.
- Improved deterministic GUI capture:
  - Added `--test-dump-after-n-apps-present-png <n> <path>` so screenshot runs can wait for multiple apps to draw at least once.
  - Updated screenshot publisher to include a multi-window screenshot (Paint + Demo) and refreshed the NetOfDots GUI golden hash.

### Milestone 22: `temple-rt` graphics API expansion (TempleOS-ish primitives)
Status (2026-02-03): Completed (added TempleOS-ish primitives in `temple-rt`, plus initial HolyC surface coverage for `GrRect`/`GrCircle`).
Acceptance:
- Add primitives commonly used by ports: line, circle, rectangle outline, blit-with-transparency.
- Add clipping/scissor so apps can draw inside UI panes.
- Add dirty-rect present (optional) for performance.

Progress notes (2026-02-03):
- Expanded `temple_rt::rt::TempleRt` with TempleOS-ish drawing primitives:
  - `draw_line(_thick)`, `draw_rect_outline(_thick)`, `draw_circle(_thick)`
  - `blit_8bpp`, `blit_8bpp_transparent`
- Added clip/scissor support in `TempleRt` via `set_clip_rect` / `reset_clip_rect`; `set_pixel`/`fill_rect` now respect the current clip rect.
- Wired the new primitives into `temple-hc`:
  - `GrLine` now uses `TempleRt::draw_line_thick` and honors `dc->thick` when no explicit `thick` arg is provided.
  - Added `GrRect` and `GrCircle` built-ins (minimal TempleOS signature coverage; angle range args are currently ignored for `GrCircle`).
- Added an end-to-end HolyC smoke test to exercise `GrRect`/`GrCircle` over the IPC harness.

### Milestone 23: Audio (TempleOS “beeps” + simple PCM)
Status (2026-02-03): Completed (added a minimal TempleOS-ish sound path: `Snd`/`Mute` over IPC to TempleShell + HolyC built-ins for `Snd`/`Beep` and Ona/Freq helpers).
Acceptance:
- A `Beep(freq, ms)`-style API exists (or equivalent).
- Optional: a PCM ring buffer API for simple music/sfx.
- Audio failure never crashes TempleShell; it degrades gracefully.

Progress notes (2026-02-03):
- Extended the IPC protocol with audio messages:
  - `MSG_SND` (set tone by TempleOS `ona`; `0` stops)
  - `MSG_MUTE` (mute on/off)
- Added `TempleRt::snd()` / `TempleRt::mute()` so both Rust apps and the HolyC runtime can control sound.
- Implemented a best-effort audio engine in TempleShell using `cpal`:
  - Lazy-initialized on first sound command.
  - Generates a simple sine tone mapped from TempleOS `Ona2Freq` (`ona=60` is `440Hz`).
  - Logs once and disables itself if no output device is available (headless/Xvfb runs don’t crash).
- Added HolyC built-ins for TempleOS sound compatibility:
  - `Snd(ona=0)`, `SndRst()`, `Mute(val)`, `IsMute()`, `Ona2Freq(ona)`, `Freq2Ona(freq)`
  - `Beep(ona=62, busy=FALSE)` implemented in TempleOS style (`Snd` + sleeps; `busy` currently ignored).
- Added end-to-end IPC tests that assert the sound messages are emitted (`run_snd_smoke_over_ipc`, `run_mute_smoke_over_ipc`).

### Milestone 24: HolyC compatibility roadmap (from “tiny subset” to “real ports”)
Status (2026-02-12): Completed (HolyC language + minimal stdlib coverage is now sufficient for multiple unmodified upstream demos/apps; compatibility work continues under later milestones).
Acceptance:
- Expand language features: `for`, `switch`, arrays, pointers, structs, enums, bit ops, char literals, escapes.
- Add basic standard library: strings, memory, math helpers (as needed for ports).
- Add multi-file programs with an `#include`-like mechanism (or TempleOS-ish file include).

Progress notes (2026-02-01):
- Added `for (...)` loops, `switch` (including TempleOS `start:`/`end:` “sub_switch” grouping), `break`/`continue`, and `++/--`.
- Added end-to-end IPC tests that run unmodified upstream demos and assert their text output:
  - `::/Demo/NullCase.HC`
  - `::/Demo/SubSwitch.HC`

Progress notes (2026-02-03):
- Added TempleOS-ish keyboard input surface for ports:
  - `GetChar(_scan_code=NULL, echo=TRUE, raw_cursor=FALSE)` built-in (minimal implementation; `_scan_code`/`raw_cursor` currently ignored).
  - Added common TempleOS `CH_*` constants (`CH_ESC`, `CH_SHIFT_ESC`, `CH_BACKSPACE`, etc.) to the HolyC environment.
  - Added keycode mapping so `ScanChar`/`NextKey`/`PressAKey` behave closer to TempleOS:
    - `KEY_ESCAPE` → `CH_ESC` / `CH_SHIFT_ESC` (tracked via `KEY_SHIFT` state)
    - `KEY_ENTER` → `'\n'`, `KEY_BACKSPACE` → `CH_BACKSPACE`
    - Ctrl-letter mapping: `Ctrl+C` → `CH_CTRLC` (`3`) and similar (`1..26`).
- Added an end-to-end IPC test asserting the keycode mapping for `GetChar`.

Progress notes (2026-02-03):
- Added HolyC language compatibility improvements needed by more upstream demos:
  - bitwise ops: `&`, `|`, `^`, `~`, `<<`, `>>`
  - adjacent string literal concatenation (`"a" "b"` → `"ab"`)
  - DolDoc-friendly string lexing so `$LK,"...",A="..."$` inside strings parses (embedded quotes no longer terminate strings).
- Added TempleOS-style 0-arg function calls in expressions when no variable exists (e.g. `RandI16` used as an expression).

Progress notes (2026-02-12):
- Added HolyC `enum { ... }` parsing (explicit `name=value` and implicit auto-increment), lowering to `I64` declarations.
- Added an end-to-end regression test covering enum value semantics (`run_enum_smoke_defines_values`).

### Milestone 25: HolyC “build/run” UX (developer loop)
Status (2026-02-12): Completed (editor-friendly diagnostics + tight build/run loop in `temple-edit` via `temple-hc --check` + `temple-hc <file>`).
Acceptance:
- `hc <file>` runs a HolyC program; errors show file/line/column.
- Editor can jump to error locations and re-run quickly.
- Optional: caching so repeated runs are fast (where applicable).

Progress notes (2026-02-12):
- `temple-hc --check <file>` prints diagnostics as `file:line:col: message` and exits `2` on parse errors, making it easy to integrate with tooling.
- `temple-edit` hotkey loop: `F5` saves (if needed), runs `--check`, jumps to the error location (cursor + selection), and launches `temple-hc <file>` when the check succeeds.
- TempleShell adds `hc`/`holyc` as an alias for `tapp hc` so running HolyC programs from the shell prompt matches the “`hc <file>`” workflow.

### Milestone 26: TempleOS API compatibility layer (names + constants)
Status (2026-02-12): Completed (documented the API surface + added a Rust-side TempleOS-ish helper module in `temple-rt`).
Acceptance:
- Provide a “TempleOS-like” module in `temple-rt` (or in HolyC built-ins) with familiar names (e.g. `Gr*`, `Key*`, etc.).
- Document the mapping and the intentional differences.

Progress notes (2026-02-12):
- Added `temple_rt::templeos` (Rust) with TempleOS-ish constants + minimal `CDC`/`CBGR48` and small `Gr*`/sound/settings shims.
- Added `TEMPLEOS_API.md` documenting where the TempleOS-ish APIs live (`#define` table vs built-ins vs Rust helpers) and calling out intentional differences.

### Milestone 27: Port showcase apps/games (Temple vibe)
Status (2026-02-12): Completed (`tapp` includes a small showcase set spanning sound + mouse + text-heavy UX).
Acceptance:
- At least 3 additional Temple-style apps/games are runnable from `tapp`.
- At least one uses sound, one uses mouse, and one is text-heavy (docs/editor integration).

Progress notes (2026-02-12):
- Added `tapp sounddemo` (TempleLinux HolyC app) as a simple “press 1..8 to beep notes” sound showcase.
- Mouse showcase: `tapp keepaway` (upstream KeepAway game).
- Text-heavy showcase: `tapp timeclock` and `tapp edit` (editor integration + workflows).

### Milestone 28: “OS polish” (boot feel, performance, robustness)
Status (2026-02-12): Completed (added scriptable shutdown + interactive screenshot export; ESC already exits from shell).
Acceptance:
- Fast startup (no obvious stalls); clear status/logging when things fail.
- Clean shutdown behavior (ESC or a `shutdown` command).
- Screenshot/export of the internal framebuffer (PNG) for sharing/debugging.

Progress notes (2026-02-12):
- Added `shutdown`/`exit` commands to quit TempleShell without needing ESC.
- Added `screenshot`/`shot [path.png]` to export the current internal framebuffer to a PNG under the Temple root (default: `/Home/screenshot-<nanos>.png`).

### Milestone 29: Vendor TempleOS sources (assets + reference)
Status (2026-01-31): Completed (cloned TempleOS sources to `third_party/TempleOS/` for palette/font/assets reference).
Acceptance:
- TempleOS source tree exists locally under `third_party/TempleOS/`.
- We can point to exact sources for:
  - default 8×8 font (`third_party/TempleOS/Kernel/FontStd.HC`)
  - default 16-color palette (`third_party/TempleOS/Adam/Gr/GrPalette.HC`)

### Milestone 30: Host visuals match TempleOS (font/palette/resolution)
Status (2026-01-31): Completed (TempleShell now defaults to 640×480 and uses TempleOS `sys_font_std` + `gr_palette_std` extracted at build time from `third_party/TempleOS` via `build.rs`).
Acceptance:
- TempleShell renders text using `sys_font_std` (TempleOS font), not an approximation.
- TempleShell uses TempleOS palette values (`gr_palette_std`) by default.
- Host-side assets are **generated/extracted from `third_party/TempleOS`** at build time (no hand-copied tables).
- TempleShell can run with an internal 640×480 mode that scales pixel-perfect.
- Optional: host UI uses TempleOS border/line-drawing characters (from the TempleOS sources) for window frames.

### Milestone 31: HolyC runtime v1 (TempleOS app execution)
Status (2026-02-01): Completed (expanded `temple-hc` to run real upstream TempleOS `.HC` sources over the TempleShell IPC protocol, with tests that execute unmodified upstream demos end-to-end).
Acceptance:
- The runtime can load a `.HC` entrypoint from `third_party/TempleOS/` and execute it.
- `#include` works for `.HC`/`.HH` within the TempleOS tree (correct search paths, no manual copying).
- Language coverage is good enough to run at least one TempleOS demo/game without editing its source.
- Failures produce clear errors (file/line/col when possible).

Progress notes (2026-02-01):
- Added HolyC literals needed for upstream demos: float literals, char/multi-char literals, and hex/binary ints.
- Added TempleOS-style print statements (`"..." ;`, `"...", args;`, `'\n';`) and a small TempleOS `Print` fmt subset:
  - commas via `,` flag or `h<aux>` on `%d`/`%u`
  - engineering `%n` with `h?` (auto SI) and `h<exp>` (fixed SI)
  - repeated-char `%h25c` / `%h*c`
- Added HolyC “call by name” statement support (`Func;`) and user-defined 0-arg function calls.
- Added unit tests that parse `third_party/TempleOS/Demo/Print.HC`, verify `#include` expansion from `::/`, and validate the fmt helpers.
- Added `for (...)`, `switch` (including TempleOS `start:`/`end:` “sub_switch”), `break`/`continue`, and `++/--` so more upstream HolyC demos run unmodified.
- Added an end-to-end IPC harness test (fake `TEMPLE_SOCK`) to run upstream demos over the real protocol without needing a GUI:
  - `::/Demo/Print.HC`
  - `::/Demo/NullCase.HC`
  - `::/Demo/SubSwitch.HC`

Next (start Milestone 32):
- Pick the first small upstream graphics demo (under `::/Demo/Graphics/`) and grow HolyC/TempleOS API coverage based on what it needs (drives Milestone 32).
- Grow HolyC language support driven by real demo ports (pointers/structs, `->`/`.`, arrays, bit ops).

### Milestone 32: TempleOS API layer v1 (graphics/input/files)
Status (2026-02-12): Completed (representative upstream graphics/UI demos + apps run unmodified; core `Gr*`/input/files shims + filesystem mapping in place).
Acceptance:
- Provide enough `Gr*` + text APIs for real TempleOS programs to draw.
- Provide key/mouse input APIs expected by TempleOS apps (at least for demos/games).
- Provide a TempleOS-ish filesystem view:
  - map `::/` and other TempleOS path conventions to `third_party/TempleOS/` and/or a writable Temple root.
- All rendering uses TempleOS assets (font/palette/border chars) and 640×480 by default.

Progress notes (2026-02-01):
- Added HolyC member access (`.` and `->`) and TempleOS-style default args in calls (e.g. `GrPlot(,x,y)`).
- Added TempleOS-ish globals: `ms` (mouse state), `ScanChar`, `DCAlias`, plus standard color constants (`BLACK..WHITE`).
- Implemented initial graphics/input shims in `temple-hc`: `GrPlot`, `Refresh`, `Yield`, `DocClear`, `DCFill`.
- Added an end-to-end IPC test that runs unmodified `::/Demo/Graphics/MouseDemo.HC` (injects a fake left-click to exit).

Progress notes (2026-02-02):
- Added HolyC assignment expressions + compound assignments (`+=`, `-=`, `*=`, `/=`, `%=`) so real TempleOS-style `for (...)` loops run unmodified.
- Added pointer-style var decl parsing for TempleOS types (e.g. `CDC *dc = DCAlias;`) and a first “user type” heuristic (types starting with `C`).
- Implemented additional TempleOS-ish graphics/input built-ins in `temple-hc`: `GrLine`, `DCDel`, `PressAKey` (flushes a frame before blocking).
- Added an end-to-end IPC test that runs unmodified `::/Demo/Graphics/NetOfDots.HC` (injects a key after first present for deterministic capture).

Progress notes (2026-02-03):
- Added more HolyC + API compatibility needed by upstream demos:
  - multiple var decls in one statement (`I64 a=0,b=1;`)
  - bitwise ops (`&`, `|`, `^`, `~`, `<<`, `>>`)
  - 0-arg function calls in expression position (e.g. `RandI16` used without `()`).
- Added RNG + math helpers used by demos:
  - `Seed(seed=0)`, `RandI16`, `SignI64`, `ClampI64`
  - Added a `Fs` global object with `pix_width`/`pix_height` (plus `pix_left`/`pix_top`).
- Adjusted `Sleep()`/`Yield()` to `Present()` so VRAM-style demos update without explicitly calling `Refresh()`.
- Added TempleShell test tooling to capture a frame after N app presents:
  - `templeshell --test-dump-after-n-presents-png <n> <path>`

Progress notes (2026-02-04):
- Added HolyC preprocessor support for constant-like `#define` and macro expansion in the lexer so unmodified TempleOS sources parse (function-like macros are still skipped for now).
  - Added a built-in `#define` table for common TempleOS constants used by upstream demos (e.g. `MSG_*`, `CH_*`, `SC_*`, `SCF_*`, `WIF_*`).
- Fixed HolyC operator precedence to match upstream expectations where `<<`/`>>` bind tighter than `+`/`-` (required for masks like `1<<MSG_KEY_DOWN+1<<MSG_CMD` in `::/Demo/PullDownMenu.HC`).
- Added HolyC runtime features needed by `MsgLoop.HC` and `PullDownMenu.HC`:
  - `do { ... } while (...);`
  - `%x/%X` formatting in the TempleOS-style printf subset
  - indexing expressions (`x[i]`) plus integer “subviews” (`arg2.u8[0]` etc.)
- Added TempleOS-ish message APIs: `ScanMsg` + `GetMsg` (plus key/mouse → `MSG_*` mapping).
- Implemented a minimal pull-down menu system compatible with TempleOS `MenuPush()` strings:
  - `MenuPush`, `MenuPop`, `MenuEntryFind`
  - menu bar + dropdown overlay rendering integrated into `Present()`
- Added upstream demo smoke coverage:
  - `::/Demo/MsgLoop.HC`
  - `::/Demo/PullDownMenu.HC` (menu click produces `MSG_CMD`)

Progress notes (2026-02-08):
- Implemented lexer built-ins for `__DIR__` and `__FILE__` so they expand per-source-file to Temple-style paths (e.g. `/Apps/Logic`), rather than host paths.
  - This also avoids breaking on TempleOS header macros like `#define __DIR__ #exe{...}` and `#define __FILE__ #exe{...}` (TempleLinux doesn’t execute `#exe` yet).
- Implemented a real `Cd(dirname=NULL, make_dirs=FALSE)` built-in (returns Bool), which updates the HolyC VM cwd and `Fs->cur_dir`.
  - Relative filesystem APIs now resolve against the VM cwd, so the common `Cd(__DIR__)` pattern makes relative `FileRead`/`FileFind` work as in TempleOS.
  - Writes remain confined to `TEMPLE_ROOT` (vendored TempleOS tree stays read-only).
- Added regression tests for `__DIR__`/`__FILE__` lexing and `Cd` + relative `FileFind` (both Temple root and TempleOS tree dirs).

Progress notes (2026-02-12):
- Marked Milestone 32 complete: end-to-end coverage now spans multiple unmodified upstream demos/apps (graphics/UI + input + filesystem patterns) with both protocol-level and GUI-level regression testing.

### Milestone 33: TempleOS tree loader + app launcher
Status (2026-02-04): Completed (treat `third_party/TempleOS` as a runnable corpus: browse/search and launch apps/demos/docs via `tapp`).
Acceptance:
- `tapp list` shows runnable apps/demos discovered from the TempleOS tree.
- `tapp run <path-or-alias>` runs a program from `third_party/TempleOS` without patching its source.
- Crashes/exceptions in an app do not crash TempleShell (process isolation or strong sandboxing).

Progress notes (2026-02-04):
- Added TempleOS tree discovery in TempleShell (prefers `TEMPLEOS_ROOT`, otherwise searches for `third_party/TempleOS` or `/usr/share/templelinux/TempleOS`).
- Extended `tapp` with TempleOS program UX:
  - `tapp tree` shows the discovered TempleOS root.
  - `tapp list` now includes discovered TempleOS programs from `Demo/` + `Apps/`.
  - `tapp search <text>` searches by alias/path.
  - `tapp run <alias|path>` resolves to a `::/…` spec and launches via `tapp hc`.
- App launching now forwards `TEMPLEOS_ROOT` to child apps so `temple-hc` can reliably resolve `::/` includes even when TempleShell’s CWD is under `~/.templelinux/`.

### Milestone 34: DolDoc/`.DD` support (docs look like TempleOS)
Status (2026-02-05): Completed (TempleShell can open and view TempleOS `.DD` docs with basic DolDoc rendering and interactive `$LK` links).
Acceptance:
- `help` / docs viewer can open `.DD` files directly from `third_party/TempleOS/Doc/`.
- Basic DolDoc markup works (colors, links, headings, simple widgets as needed by real docs).
- Link targets resolve correctly (e.g. TempleOS-style `$LK` links open the referenced doc/source).

Progress notes (2026-02-04):
- Extended `help <topic>` to fall back to the vendored TempleOS tree (`third_party/TempleOS`) and open `::/Doc/<topic>.DD` (and `help ::/...` for explicit specs).
- Added a minimal DolDoc renderer that handles the most common tags seen in real docs:
  - `$FG`/`$BG` color changes (+ reset)
  - `$TX` (including `+CX` centering)
  - `$LK` link text (rendered in link color)
  - `$TR` headings and `$ID` indentation
- `help` now opens an in-terminal doc viewer mode:
  - Scroll with arrows / PgUp/PgDn; Esc exits.
  - Tab cycles links; Enter opens the selected link target.
  - Mouse: click selects a link; double-click opens.
- Non-`.DD` targets opened via links (e.g. `::/Demo/.../*.HC`) display as plain text (useful for following links into source).
- Updated `packaging/bin/templelinux-publish-screenshots` to include a DolDoc screenshot (`help DemoIndex`).

Progress notes (2026-02-05):
- Improved `$LK` parsing to honor `A="..."` link targets (the common TempleOS pattern), so clicking links like:
  - `A="FI:::/Doc/HolyC.DD"` (file) and `A="FF:::/Doc/Glossary.DD,AOT Compile Mode"` (file+section)
  - `A="MN:Load"` / `A="HI:Hash"` (manual-node style links)
  opens something meaningful instead of doing nothing.
- Added `$AN` anchor support and "jump" behavior (anchors + section-string matching) for file+section links.
- Implemented additional common Doc tags for better visual fidelity:
  - `$BK` (blink → approximated with a strong background)
  - `$IV` (invert), `$HL` (syntax-highlight tint), `$UL` (underline tint)
  - `$MA`/`$MA-X` treated as clickable links (useful in `DemoIndex.DD`).
- Implemented cursor-movement tags used for simple “table/diagram” layouts:
  - `$CM` (basic X/Y cursor moves with `+LX`/`+RX`)
  - `$CM-RE` (X-only cursor move used heavily by `Doc/Job.DD`).
- Stopped Doc/source parsing at the first `NUL` byte to avoid showing embedded sprite/binary tails stored after the text terminator in some TempleOS documents.
- Added minimal placeholders for `$IB` blocks so binary tags don’t silently disappear (`[bin]`).
- Updated the X11 GUI golden hash for `help DemoIndex` after expanded link coverage.

Progress notes (2026-02-06):
- Implemented real `$SP` sprite rendering in the DolDoc viewer by parsing appended `CDocBin` tails (NUL-terminated text + bin records) and drawing TempleOS `CSprite` elements inline (clipped to the doc viewport).
- Updated `packaging/bin/templelinux-publish-screenshots` to include a sprite-heavy DolDoc screenshot (`help HelpIndex`) and a HolyC sprite demo (`tapp hc ::/Demo/Graphics/SpritePlot.HC`).
- Improved DolDoc text fidelity for “meta” docs that describe DolDoc syntax:
  - `$$` now renders a literal `$`.
  - `$$$...$$$` now renders a literal DolDoc cmd string (e.g. `$FG,RED$`) instead of being parsed/executed.
  - This makes docs like `::/Doc/DolDocOverview.DD` render much closer to TempleOS.
- Updated `packaging/bin/templelinux-publish-screenshots` to include `help DolDocOverview` as `doldoc-overview.png`.

Progress notes (2026-02-13):
- Fixed DolDoc bin-tail parsing for docs like `::/PersonalMenu.DD` that contain multiple sprite bins: recover bin boundaries by scanning for plausible headers and validating `CSprite` blobs instead of trusting the stored `CDocBin.size` field.
- Updated the screenshot gallery after the fix (PersonalMenu icons now render reliably): https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/13/093920-199042/index.html
- Implemented safe, deny-by-default DolDoc `$MA`/`$MA-X` `LM="..."` actions:
  - interpret the common `Cd("::/...");Dir;View;` macro pattern as “open a read-only directory listing view”
  - support `KeyMap;View;` by opening `::/Doc/KeyMap.DD`
  - this makes directory entries in docs like `::/Doc/DemoIndex.DD` actually navigable without executing arbitrary HolyC
- Implemented additional DolDoc “widget-ish” tags for better real-doc fidelity:
  - `$HC` HTML blocks now render as a small placeholder with the first `http(s)://...` URL exposed as a safe, clickable `browse` action (opens via `xdg-open`).
  - `$SO` embedded Psalmody songs now render as clickable links (currently “song: unsupported” on open; playback is a separate future task).
- Improved `$IB` bin handling in the DolDoc viewer:
  - If the `$IB` tag text is empty (common when used as an “inline binary”), and `BI=<n>` resolves to a `CDocBin`, render it as an inline sprite instead of showing a `[bin]` placeholder.
  - If tag text is present (e.g. `<1>`), keep rendering it as text (so code/docs that reference bins don’t get visually spammy).
- Uploaded an updated screenshot gallery capturing the PersonalMenu “desktop” and representative apps/demos after the `$IB` rendering change: https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/13/195528-3114048/index.html
- Fixed a regression where `::/PersonalMenu.DD` sprites mostly fell back to text labels (bin-tail parsing stopped early due to overly-permissive “next header” scanning):
  - Parse the doc’s text first, extract the expected `BI=<n>` set, and use it to filter plausible `CDocBin` headers.
  - Resync when encountering corrupt records so later bins can still be recovered.
  - Updated screenshot gallery after the fix: https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/13/213414-99310/index.html

Progress notes (2026-02-14):
- Fixed DolDoc sprite layout issues in `::/PersonalMenu.DD` (overlapping / off-screen icons):
  - Reserve sprite width/height using `SpriteBounds::width()/height()` (the previous code incorrectly used `x1/y1` directly, which is wrong when the sprite has negative extents).
  - Draw sprites offset by `-bounds.x0/-bounds.y0` so icons anchor at the correct “top-left” position in the DolDoc grid.
- Uploaded an updated screenshot gallery after the layout fix: https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/14/002701-2181245/index.html
- Fixed a regression in DolDoc `CDocBin` tail parsing that caused `::/PersonalMenu.DD` icons to be missing/repeated/mispositioned:
  - Parse bins sequentially and recover record boundaries by scanning for plausible “next header” offsets, instead of independently “best-effort” recovering each bin by number.
  - Validate sprites more strictly (must parse from offset 0) and repair a common TempleOS quirk where some sprite bins omit the leading `SPT_BITMAP` type byte.
- Uploaded a fresh screenshot gallery after the bin-tail parsing fix: https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/14/024441-3930551/index.html
- Fixed TempleOS `SPT_SHIFTABLE_MESH` icon rendering (notably mesh-based PersonalMenu icons) where vertex coordinates/indices/colors are stored with packed/shifted bytes:
  - Decode common “mostly 0x00/0xff with one informative byte” patterns to recover small signed coords (avoids the classic `0x00ffffff → 255` mis-decode that caused huge triangles).
  - Apply the decode in both sprite bounds calculation and mesh rendering to prevent icon overlap and on-screen artifacts.
- Uploaded a full updated screenshot gallery after the mesh decode fix: https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/14/113907-2319094/index.html

Progress notes (2026-02-15):
- Fixed a remaining `::/PersonalMenu.DD` mesh sprite artifact where some packed coords decode to “reasonable-range” `I32`s (e.g. `0x000000ff → 255`, `0xffffff00 → -256`) and bypass the mesh-coordinate unpacking heuristics, causing tall glitch triangles that overwrite icon labels in the DolDoc viewer.
- Added a GUI golden test for `help FF:::/PersonalMenu.DD,X-Caliber` so regressions in PersonalMenu sprite/layout rendering are caught in CI (when `TEMPLE_GUI_TESTS=1` is set).

Next:
- Expand DolDoc layout/widget tags as needed by more real docs (tables/columns, widgets, and richer formatting):
  - Optional: support Psalmody playback for `$SO` songs.
- If we encounter more CP437 quirks: extend decode/encode handling (we now have a full CP437 mapping + a non-UTF8 decode fallback, but TempleOS sources sometimes have surprises).

### Milestone 35: Adam-like editor story (TempleOS dev loop)
Status (2026-02-04): Completed (TempleLinux has an Adam-like dev loop: edit → run → jump-to-error, while treating the vendored TempleOS tree as read-only).
Acceptance:
- A TempleOS-like editor exists in the environment (either by running TempleOS editor sources or a compatible reimplementation).
- From the editor, run a HolyC program and jump to errors.
- Works on files in a writable Temple root plus the vendored read-only TempleOS tree.

Progress notes (2026-02-04):
- `temple-hc` gained a `--check` mode that compiles without connecting to TempleShell and prints parse errors as `file:line:col: message` (easy to consume by tools).
- `temple-edit` gained an editor-driven run loop:
  - `F5` runs the current file by invoking `temple-hc --check <file>` first.
  - On compile errors, the editor jumps to the first diagnostic and highlights the location.
  - On success, the editor launches `temple-hc <file>` as a separate Temple app window (so you can Alt+Tab between editor and running program).
- TempleOS sources are treated as read-only:
  - `edit ::/…` opens files from the vendored TempleOS tree.
  - `temple-edit` shows `[RO]` and blocks modifications/saves for files under `TEMPLEOS_ROOT`.

### Milestone 36: “LinuxBridge” HolyC app (host integration)
Status (2026-02-05): Completed (TempleOS-style HolyC app that launches modern Linux apps/URLs/files without breaking the Temple look).
Acceptance:
- From within the Temple environment, `LinuxBridge` can:
  - open a URL on Linux (`browse`/`xdg-open`, launching Chrome/Firefox),
  - open a file on Linux (`xdg-open`),
  - launch a Linux command (`run` / configured allowlist).
- The UI is TempleOS-native (rendered with TempleOS assets, not a modern widget toolkit).
- The bridge fails safe (invalid commands do nothing; no host crash).

Progress notes (2026-02-05):
- Added TempleLinux-specific HolyC built-ins to `temple-hc`:
  - `LinuxBrowse("url")` → `xdg-open <url>`
  - `LinuxOpen("path")` → `xdg-open <path>` with TempleLinux path mapping:
    - `/Home/...` maps to `TEMPLE_ROOT/Home/...`
    - `::/Demo/...` maps to the vendored TempleOS tree
  - `LinuxRun("cmd ...")` → spawn a Linux command (deny-by-default allowlist)
  - `LinuxLastErr()` → last host-bridge error string (for UI)
  - `GetStr(msg,dft,flags)` → minimal prompt helper for in-app text entry
- `LinuxRun` allowlist config:
  - env: `TEMPLE_LINUX_RUN_ALLOW="chrome,firefox,foot"` (comma/whitespace separated)
  - or file: `/Cfg/LinuxRunAllow.txt` (one program per line)
- Added `LinuxBridge.HC` (TempleOS-style UI) under `holyc/LinuxBridge.HC`, bootstrapped into the writable Temple root at `/Apps/LinuxBridge.HC`.
- Added `tapp linuxbridge` (and `tapp bridge` alias) + a file-browser shortcut entry.

### Milestone 37: “Run TempleOS apps” compatibility definition + smoke tests
Status (2026-02-07): Completed (compatibility target + representative smoke list documented, with both protocol-level and GUI-level regression coverage).
Acceptance:
- Document a compatibility target:
  - “A representative set of TempleOS apps/demos/docs run on TempleLinux with correct visuals and usable input.”
- Maintain a smoke list of representative programs from `third_party/TempleOS` (editor/docs, a graphics demo, a game, a file manager, etc.).
- Provide a repeatable manual checklist (MVP) and an automated harness where feasible:
  - **Protocol-level harness**: run TempleOS `.HC` demos against a fake `TEMPLE_SOCK` and assert text output + presents (fast, no GUI).
  - **GUI-level harness**: run TempleShell under a virtual framebuffer and take deterministic screenshots for visual regression (slower, validates winit/wgpu + rendering).

Progress notes (2026-02-01):
- Added automated end-to-end tests that execute unmodified upstream demos via a fake `TEMPLE_SOCK`:
  - `::/Demo/Print.HC`
  - `::/Demo/NullCase.HC`
  - `::/Demo/SubSwitch.HC`
- Added a first upstream graphics demo smoke test:
  - `::/Demo/Graphics/MouseDemo.HC` (injects a fake left-click to exit)

Progress notes (2026-02-02):
- Added a second upstream graphics demo smoke test:
  - `::/Demo/Graphics/NetOfDots.HC` (injects a key after the first present so the “pre-exit” frame is captured deterministically).
- Extended the protocol-level fake `TEMPLE_SOCK` harness to optionally deliver input events after the first `Present()` (avoids races for demos that clear immediately after input).
- Added first GUI smoke tooling (headless) to validate real TempleShell rendering:
  - TempleShell can now dump its internal framebuffer to a PNG in a deterministic test mode.
  - Added X11 + Wayland smoke scripts under `packaging/bin/` to run TempleShell headlessly and capture a PNG.
  - Added an optional `cargo test` GUI smoke test (gated behind `TEMPLE_GUI_TESTS=1`) that runs `NetOfDots` under Xvfb and verifies the PNG dimensions.

Progress notes (2026-02-05):
- Added a protocol-level smoke test for TempleLinux’s `LinuxBridge.HC` (exits on Esc after first `Present()`).
- Adjusted `temple-hc` execution so top-level declarations run in a persistent “global” scope (fixes apps with globals + `Main()`), while avoiding double-running `Main()` when top-level already calls it.
- Implemented HolyC semantics required by unmodified TempleOS programs:
  - chained comparisons (e.g. `100<=x<400`, `player==board[0]==board[1]==board[2]`)
  - short-circuit `&&` / `||` (critical for safe out-of-range avoidance in real TempleOS code)
- Added a protocol-level smoke test for an unmodified upstream game:
  - `::/Demo/Games/TicTacToe.HC` (injects Ctrl+Alt+C after first `Present()` to exit via `try/catch`).
- Expanded GUI golden coverage (X11/Xvfb, gated by `TEMPLE_GUI_TESTS=1`) to better represent the “full TempleOS UI” beyond graphics:
  - file browser (`files /Home/Demo`)
  - DolDoc viewer (`help DemoIndex`)
  - LinuxBridge (`tapp linuxbridge`)
  - editor (`tapp edit /Home/EditorDemo.txt`)
  - PullDownMenu (`tapp hc ::/Demo/PullDownMenu.HC`, with deterministic hover injection)
  - multi-window layout (Paint + Demo)
  - all captured as deterministic 640×480 PNGs with pinned SHA-256 in `tests/gui_smoke.rs`.
  - Test-mode status line now redacts the focus/browser state to avoid nondeterministic golden changes.
- Began the first upstream `::/Apps/` target (`::/Apps/TimeClock/…`) by extending `temple-hc` parsing + runtime so upstream HolyC can compile and run without modifications:
  - Parser: `class`/`extern class`, `public` function qualifiers, unary `*` deref, and HolyC cast syntax like `ptr(CDate *)++`.
  - Runtime: minimal heap + pointer deref for `U8*`/`CDate*`, `MAlloc`/`CAlloc`/`Free` (typed class allocation when called with `sizeof(Class)`), queues (`QueInit/QueIns/QueRem`), file ops (`FileRead/FileWrite/FileFind/DirMk` with `~/` mapping), and date/time (`Now`, `CDate` `.date`/`.time`, plus `%D/%T` formatting).
  - `GetStr(...)` now returns a HolyC-compatible `U8*` heap string pointer and implements `GSF_WITH_NEW_LINE` semantics:
    - Enter inserts `\n` (multi-line editing)
    - Esc finishes/accepts
    - Shift+Esc returns an empty string
  - Added protocol-level tests:
    - `::/Apps/TimeClock/Load.HC` installer runs and creates `~/TimeClock/`
    - `TimeRep(NULL)` on an empty file prints a deterministic `Week Total:00:00:00` line.
    - `TimeRep(NULL)` on a deterministic 2-entry file prints `Week Total:08:30:00`.
    - `GetStr(,,GSF_WITH_NEW_LINE)` newline/exit behavior is deterministic under injected key events.
- Improved TempleOS-ish text fidelity for upstream apps:
  - `temple-hc` now interprets a small DolDoc subset in printed strings: `$$COLOR$$`, `$$FG$$`, and `$$BK,<idx>$$` (background).
- Made upstream TimeClock usable as a real app session (without needing a HolyC REPL):
  - Added `holyc/TimeClock.HC` wrapper (interactive menu for `PunchIn`/`PunchOut`/`TimeRep` while reusing unmodified upstream `::/Apps/TimeClock/*`).
  - Integrated it into TempleShell as `tapp timeclock` (plus a file-browser shortcut entry).
  - Added a protocol-level smoke test for the wrapper UI (also validates DolDoc markup stripping).
  - Added a TimeClock screenshot capture + gallery entry to `packaging/bin/templelinux-publish-screenshots`.

Progress notes (2026-02-06):
- Added HolyC support for NUL-terminated `.HC` files with appended DolDoc bins, and expression support for `$IB` (bin ptr) + `$IS` (bin size) to match TempleOS behavior.
- Implemented `Sprite3(...)` as a HolyC builtin using the shared TempleOS sprite renderer, enabling unmodified `::/Demo/Graphics/SpritePlot.HC`.
- Added a protocol-level smoke test for `::/Demo/Graphics/SpritePlot.HC` (captures an early sprite frame and asserts non-zero pixels + non-zero `$IS` size).
- Fixed CP437 source fidelity for vendored TempleOS programs:
  - `temple-hc` preprocessing now preserves raw source bytes (no lossy UTF-8 conversion), which is required for CP437-heavy files like `::/Kernel/KMain.HC` (box-drawing char literals, border chars, etc.).
  - Added a full CP437 Unicode↔byte mapping in `temple-rt` (`assets::encode_cp437` + decode helpers) and used it for string/DolDoc decoding and doc viewing.
  - Updated the HolyC lexer to treat bytes 128-255 as “letters” inside identifiers (TempleOS behavior; enables sources like `::/Demo/ExtChars.HC` to tokenize correctly).
  - Added a regression test to ensure CP437 bytes in char literals survive preprocessing.

Progress notes (2026-02-07):
- Documented the compatibility definition + representative smoke list in `COMPATIBILITY.md` (fast protocol-level tests + GUI goldens + screenshot publishing workflow).
- Improved TempleOS app file I/O compatibility: absolute `/...` paths now fall back to the vendored TempleOS tree for **reads** when missing from `TEMPLE_ROOT`, while **writes** remain confined to `TEMPLE_ROOT` (prevents accidentally mutating `third_party/TempleOS`). Added a protocol-level regression test.
- Extended HolyC cast syntax to support non-pointer “type parens” casts like `expr(U64)` (required for unmodified `::/Demo/ExtChars.HC`).
- Implemented a minimal TempleOS `text` global with `text.font` and live glyph updates so demos can patch font glyphs at runtime (again required by `::/Demo/ExtChars.HC`).
- Fixed CP437 fidelity around glyph 255 by allowing Unicode NBSP (`U+00A0`) to roundtrip back to CP437 byte `0xFF` (TempleOS uses that glyph slot for extended-char tricks).
- Added a protocol-level smoke test for `::/Demo/ExtChars.HC` that asserts the patched glyph draws non-blank pixels.
- Lexer: treat bytes 128–255 as valid identifier *starts* (not just identifier continuations), enabling sources that use accented CP437 variables like `é`/`è`/`ê` (`::/Demo/Graphics/Box.HC`).
- Parser/runtime: support call expressions on non-ident callees (e.g. `(*fn_ptr)(arg)`), plus a `FuncRef` value for `&Function` pointers (required for controls/callback-heavy sources like `::/Demo/Graphics/Slider.HC`).
- Parser/runtime: support additional compound assignments used heavily upstream: `&=`, `|=`, `^=`, `<<=`, `>>=` (fixes parsing of demos like `::/Demo/Graphics/Grid.HC`).
- Parser: accept C-style function-pointer variable declarations like `U0 (*cb)(CDC *dc,I64 x,I64 y);` (also required by `::/Demo/Graphics/Grid.HC`).
- HolyC lexer: allow literal newline bytes inside `'...'` char literals (TempleOS behavior). This fixes `MenuPush` specs that embed `'\n'` inside strings.
- HolyC expression precedence: align with TempleOS operator precedence (`<<`/`>>` bind tighter than `*`, and `&`/`^`/`|` bind tighter than `+`/`-`). Added protocol-level tests to lock this in.
- Upstream demo/game: `::/Demo/Games/CharDemo.HC` now runs unmodified under TempleShell (menu specs, `goto`, `Fs->draw_it` render path, key mapping).
- Updated `packaging/bin/templelinux-publish-screenshots` to capture `chardemo.png`.
- HolyC parser/runtime: support the TempleOS pattern `class Foo { ... } foo;` (instance declarations after the class body), required by unmodified `::/Demo/Graphics/Slider.HC`.
- HolyC VM: treat `CAlloc(sizeof(CFoo))` for unknown user types as allocating struct-like objects; allow member access through `&var` pointers (`VarRef`); extend `MemSet`/`MemCpy` to operate on struct objects (common TempleOS pattern for saving/restoring globals).
- Implemented minimal TempleOS-ish “ctrl” plumbing:
  - `Fs->last_ctrl` queue head and control list drawing on each `Present()`
  - `TaskDerivedValsUpdate` iterates ctrls and calls `c->update_derived_vals`
  - `GrPrint` for coordinate-based formatted text output
- Upstream demo/UI: `::/Demo/Graphics/Slider.HC` now runs unmodified; added a protocol-level smoke test.
- Updated `packaging/bin/templelinux-publish-screenshots` to capture `slider.png`.
- Added minimal task scrollbar state + stubs needed by upstream “window-manager-ish” demos:
  - `Fs->horz_scroll` / `Fs->vert_scroll` objects (min/pos/max + simple clamping in `TaskDerivedValsUpdate(task)`).
  - `WinBorder(ON)` accepts the flag argument (no-op, but parses/runs upstream code).
  - `DocScroll` stub (no-op).
  - Added `ON`/`OFF` to the builtin `#define` set.
- Upstream demo/UI: `::/Demo/Graphics/ScrollBars.HC` now runs unmodified; added a protocol-level smoke test.
- Updated `packaging/bin/templelinux-publish-screenshots` to capture `scrollbars.png`.
- TempleShell headless test harness: added `--test-app-exit mouseleft` (sends a left click after dumping) so mouse-only demos can exit deterministically in screenshot runs.
- Upstream demo/UI: `::/Demo/Graphics/Grid.HC` now runs unmodified (grid globals + custom mouse draw overlay); added a protocol-level smoke test.
- Updated `packaging/bin/templelinux-publish-screenshots` to capture `grid.png`.
- HolyC fmt: implemented `%Z` define-list formatting plus minimal define-list support:
  - builtin `ST_COLORS` list (and `DefineLstLoad`/`DefineSub`)
  - added `COLORS_NUM`/`COLOR_INVALID`/`COLOR_MONO` to the builtin `#define` set
- TempleShell palette settings: added IPC support for palette changes + a minimal settings stack:
  - `GrPaletteColorSet` sends palette updates to TempleShell
  - `SettingsPush`/`SettingsPop` now push/pop the palette (restores original palette after demos)
- Upstream demo/UI: `::/Demo/Graphics/Palette.HC` now runs unmodified; added a protocol-level smoke test.
- Updated `packaging/bin/templelinux-publish-screenshots` to capture `palette.png`.
- HolyC memory/task stubs: added `ACAlloc` plus minimal `adam_task` / `sys_winmgr_task` globals (unblocks Adam-included wallpaper/control demos).
- HolyC ctrls: implemented mouse hit-testing + `left_click` dispatch, including `CTRLF_CAPTURE_LEFT_MS` behavior (makes Slider-like demos actually interactive with the mouse).
- Input fidelity: updated blocking input loops (`PressAKey`, `GetChar`, `GetKey`) to periodically `Present()` while waiting so mouse-driven UI updates remain visible.
- Added a TempleLinux wrapper for the non-runnable upstream wallpaper demo: `holyc/WallPaperCtrl.HC` (launch via `tapp wallpaperctrl`).
- Added a protocol-level regression test that clicks the ctrl and asserts the next frame changes (`holyc/WallPaperCtrl.HC`).
- Updated `packaging/bin/templelinux-publish-screenshots` to capture `wallpaperctrl.png` and include it in the gallery.
- TempleShell wallpaper desktop: added a “wallpaper app” session kind that renders full-screen behind normal windows.
  - Added a terminal render mode that lets wallpaper show through under black-background terminal cells while keeping text readable.
- Added a TempleLinux wrapper for the upstream wallpaper animation: `holyc/WallPaperFish.HC` (launch via `tapp wallpaperfish` / `tapp wallfish`).
  - Runs unmodified `::/Demo/Graphics/WallPaperFish.HC` as a background wallpaper layer while other apps run on top.
- HolyC runtime fixes needed by WallPaperFish:
  - builtins: `tS` (seconds since VM start) + `Tri(t,period)` + `DCSymmetrySet` stub + minimal `gr` globals used by the demo.
  - class arrays: default-initialize class-typed array elements as objects and allow `->` field access through array pointers.
- Updated `packaging/bin/templelinux-publish-screenshots` to capture `wallpaperfish.png` and include it in the gallery.
- HolyC fmt: added `%z` (string-list by index) formatting for apps like `::/Apps/Logic` that print `%z` gate names.
- DolDoc bins: improved tail parsing to stop on corrupted records and added a fallback for missing `BI=<n>` lookups so apps with truncated bin tails can still run.
- Upstream app: `::/Apps/Logic/Run.HC` now runs and is launchable via `tapp logic`; updated screenshot publishing to capture `logic.png`.
- Upstream game/app: `::/Apps/KeepAway/Run.HC` now runs unmodified (exercises sprites + float coords + registry defaults); launch via `tapp keepaway`; updated screenshot publishing to capture `keepaway.png`.
- Latest uploaded gallery (includes KeepAway + Logic + WallPaperFish wallpaper desktop): `https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/07/205220-1055058/index.html`

Next:
- Expand the representative smoke list to include at least:
  - ✅ one upstream **game** from `::/Demo/Games/` (unmodified):
    - `::/Demo/Games/TicTacToe.HC`
    - `::/Demo/Games/CharDemo.HC`
  - ✅ one upstream **app** from `::/Apps/` (unmodified source), driving HolyC + TempleOS API coverage based on what it needs.
    - Suggested first target: `::/Apps/TimeClock/TimeClk.HC` (practical “real app”, exercises file I/O + time + list management).
    - Expected missing pieces to implement before it can run:
      - ✅ `class` / `extern class` parsing (struct-like user types, including self-referential pointers).
      - ✅ memory: `MAlloc`, `CAlloc`, `Free` (byte heap + typed class allocation when called with `sizeof(Class)`).
      - ✅ pointers: unary `*` deref, HolyC cast syntax `ptr(Type *)`, and typed cast++ stride for `CDate` file parsing.
      - ✅ queues: `QueInit`, `QueIns`, `QueRem` (circular doubly-linked list).
      - ✅ file I/O: `FileRead`, `FileWrite`, `FileFind`, `DirMk` (Temple root mapping + `~/` handling).
      - ✅ time/date: `Now`, `CDate` `.date`/`.time`, plus `%D/%T` formatting support.
      - ⏭ Remaining fidelity blockers for a *fully TempleOS-faithful* TimeClock experience:
        - optional: `.Z` compression semantics (TempleOS uses a custom archive format for “compressed” files; TempleLinux currently treats `.Z` as a plain file, which is sufficient for most vendored sources but not 1:1 faithful).
  - ✅ one upstream **controls/UI** demo from `::/Demo/Graphics/` (unmodified), to better represent the “full TempleOS UI” beyond the shell:
    - `::/Demo/Graphics/Slider.HC`

- Next targets to expand the “full TempleOS UI” feel:
  - ✅ `::/Demo/Graphics/ScrollBars.HC` (exercises task scrollbars + sprite bins; headless-capturable via `GetChar`).
  - ✅ Add a new headless test exit gesture for mouse-only demos (e.g. “left click to exit”), so screenshot capture doesn’t hang on programs that ignore Enter/Esc.
  - ✅ Add a mouse-only upstream demo to the screenshot gallery + smoke list:
    - `::/Demo/Graphics/Grid.HC`

  - Next likely UI-heavy targets:
    - ✅ `::/Demo/Graphics/Palette.HC` (palette APIs: `GrPaletteColorSet`, `%Z` define lists, and settings push/pop).
    - ✅ `::/Demo/Graphics/WallPaperCtrl.HC` (via TempleLinux wrapper `tapp wallpaperctrl`; more ctrl patterns + mouse interactions).

### Milestone 38: Headless GUI regression testing (Xvfb + Wayland headless)
Status (2026-02-02): Completed (use local virtual framebuffer support for X11/Wayland to run TempleShell in automated tests and catch visual regressions).
Acceptance:
- A single command runs TempleShell headlessly and exits deterministically (no manual window management).
- TempleShell can dump its internal framebuffer to a PNG for tests (or we can use compositor screenshot tooling, but internal dump is preferred).
- Provide scripts for both backends:
  - X11: `xvfb-run` (Xvfb virtual framebuffer; if you have a local wrapper like `fvxb`, use that too)
  - Wayland: `weston --backend=headless-backend.so` (Wayland headless compositor; or `fvxb` if it provides a Wayland VFB wrapper)
- Add a small set of golden-image tests (or a pixel-diff tolerance-based comparator) for:
  - terminal text rendering (font/palette correctness)
  - at least one upstream HolyC demo output frame
- Document required env vars for headless wgpu if needed (e.g. software Vulkan / backend selection).

Progress notes (2026-02-02):
- Added a TempleShell test mode to dump the internal framebuffer to PNG:
  - `templeshell --test-dump-initial-png <out.png>` (dump shell/terminal frame)
  - `templeshell --test-dump-app-png <out.png>` (wait for first app `Present()`, dump, then exit after app disconnect)
  - `templeshell --test-dump-after-n-apps-present-png <n> <out.png>` (wait for N distinct apps to `Present()`, dump, then exit)
  - `templeshell --test-dump-after-n-presents-png <n> <out.png>` (wait for N total app `Present()` messages, dump, then exit)
  - `templeshell --test-app-exit <enter|ctrlq|esc|mouseleft|none>` (controls the “exit gesture” sent after dumping; default: `enter`)
  - `templeshell --test-send-after-first-app-present <event>` (repeatable; inject deterministic input after first app `Present()`)
  - `templeshell --test-run-shell <cmd>` (repeatable; run one or more shell commands before dumping to capture specific UIs like the file browser)
- Added headless runner scripts:
  - X11: `packaging/bin/templelinux-gui-smoke-x11`
  - Wayland: `packaging/bin/templelinux-gui-smoke-wayland`
- Added a publishing helper that uploads screenshots + an `index.html` gallery via `wtf-upload`:
  - `packaging/bin/templelinux-publish-screenshots`
  - Captures: initial shell, file browser (`files`), DolDoc (`help DemoIndex`), LinuxBridge (`tapp linuxbridge`), multi-window (Paint+Demo), multi-window (Paint+Demo+NetOfDots), a standalone `NetOfDots` frame, `TicTacToe.HC`, `PullDownMenu.HC`, `Lines.HC`, `temple-edit`, and an editor error-jump frame (F5 run → jump to compile error).
  - Uses `wtf-upload --key <prefix>/<filename>` so `index.html` can reference stable relative filenames.
- Added golden screenshot regression tests (X11/Xvfb) gated behind `TEMPLE_GUI_TESTS=1`:
  - `tests/gui_smoke.rs` captures the initial TempleShell frame and the first `NetOfDots.HC` frame, then checks a pinned SHA-256 for each.
  - Test-mode status line redacts mouse/output/scale text to keep the images stable across headless environments.
- Headless notes:
  - If Xvfb crashes due to NVIDIA EGL selection, force Mesa (example): `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl …`

Progress notes (2026-02-04):
- Extended TempleShell test tooling to better capture “full TempleOS UI” interactions in headless runs:
  - `templeshell --test-app-exit esc` (send Escape after dumping and wait for app disconnect)
  - `templeshell --test-send-after-first-app-present <event>` to inject deterministic input after the first `Present()`:
    - `mouse_move:x,y`
    - `mouse_button:left,down`
    - `key:esc,down` (and other basic keys)
- Updated the screenshot publishing script to include an interactive menu demo capture:
  - `packaging/bin/templelinux-publish-screenshots` now captures `::/Demo/PullDownMenu.HC` as `pulldownmenu.png`.
- Latest uploaded gallery (includes TicTacToe + LinuxBridge + PullDownMenu + DolDoc + editor error-jump): `https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/05/042021-1463002/index.html`

Progress notes (2026-02-05):
- Silenced noisy `Broken pipe` exits from Temple apps when TempleShell intentionally exits early in headless screenshot modes (treats `EPIPE` as a graceful disconnect in `temple-demo`, `temple-paint`, and `temple-hc`).
- Updated `packaging/bin/templelinux-publish-screenshots` to:
  - capture an additional DolDoc/table-heavy doc (`help Job` → `doldoc-job.png`), and
  - upload `timeclock.png` (was previously captured but not included in the upload list).

Progress notes (2026-02-06):
- Updated the screenshot publishing script to capture `help DolDocOverview` (`doldoc-overview.png`) so we have a deterministic visual check for DolDoc “escaped '$' + literal cmd” rendering.
- Latest uploaded gallery (includes DolDocOverview + HelpIndex sprites + SpritePlot): `https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/06/221451-2162174/index.html`

Progress notes (2026-02-07):
- Updated the screenshot publishing script to cover more “full TempleOS UI” surfaces in one run:
  - UI-heavy demos: `Slider`, `ScrollBars`, `Grid`, `Palette`, `WallPaperCtrl`
  - Wallpaper desktop: `WallPaperFish` running as the background layer behind a normal window
  - Games + apps: `CharDemo`, `TicTacToe`, `TimeClock`, `Logic`, `KeepAway`
- Latest uploaded gallery (includes KeepAway + Logic + WallPaperFish wallpaper desktop): `https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/07/205220-1055058/index.html`

Progress notes (2026-02-08):
- Stabilized flaky X11 GUI golden tests by adding an optional “sync present” handshake:
  - IPC: added `MSG_PRESENT_ACK` and `Msg::present_ack(seq)`.
  - `temple-rt`: if `TEMPLE_SYNC_PRESENT=1`, `Present()` blocks until the shell acks the seq (timeout via `TEMPLE_SYNC_PRESENT_TIMEOUT_MS`, default `500ms`).
  - `templeshell`: acks app presents during the normal redraw path so dumps happen only after the frame is fully consumed.
  - Enabled `TEMPLE_SYNC_PRESENT=1` in `tests/gui_smoke.rs` and `packaging/bin/templelinux-publish-screenshots`.
- Made the PullDownMenu golden capture deterministic by injecting a single menu-bar hover move (`mouse_move:1,0`) instead of a hover+dropdown move (avoids timing-dependent “second move” races).
- Made `temple-hc` treat “sync present” disconnects as a graceful `Broken pipe` exit (avoids noisy errors when TempleShell intentionally exits early after dumping PNGs).
- Latest uploaded gallery: `https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/08/072528-1791502/index.html`

Progress notes (2026-02-17):
- Ran a full GUI smoke + screenshot gallery capture via `packaging/bin/templelinux-publish-screenshots`:
  - Latest uploaded gallery: `https://tmp.uh-oh.wtf/templelinux/screenshots/2026/02/17/120110-4137273/index.html`
  - Committed the captured PNGs + `index.html` under `docs/screenshots/2026-02-17/` and updated `README.md` to reference them.
- Tweaked the screenshot publisher so editor sample files are created in a temp dir (keeps `docs/screenshots/.../` clean).
- Uploaded `research.md` to Open WebUI via `chat-uh-oh-from-file`: `https://chat.uh-oh.wtf/c/578f7929-e2d0-49e2-a415-d561670923d3`

### Milestone 39: Distribution packages (Arch + Ubuntu)
Status (2026-02-17): Completed (priority; “install on top of” existing distros)

Goal:
- Make TempleLinux installable as a normal user-space package on common distros (no custom ISO required).

Acceptance:
- Arch Linux:
  - Provide an AUR `templelinux-git` (or similar) package that installs:
    - `templeshell`, `temple-hc`, `temple-edit`, `temple-paint`, `temple-demo` to `/usr/bin/`.
    - `templelinux-session` to `/usr/bin/` (or `/usr/lib/templelinux/` + a small `/usr/bin/` wrapper).
    - `packaging/wayland-sessions/templelinux.desktop` to `/usr/share/wayland-sessions/`.
    - The TempleOS source/assets tree to `/usr/share/templelinux/TempleOS/` so `TEMPLEOS_ROOT` auto-discovery works out of the box.
  - Document required deps + optional deps (sway session, XWayland, test tooling).
- Ubuntu/Debian:
  - Provide a `.deb` (either via `cargo-deb` or `debian/` packaging) that installs the same filesystem layout.
  - Ensure runtime deps are accurate (Wayland/X11 libs, GL/EGL/Vulkan stack, ALSA, `xdg-open`, etc).
- Post-install workflow:
  - `templeshell` runs from an existing desktop session without needing `cargo`.
  - The login manager shows “TempleLinux” as a Wayland session (via `/usr/share/wayland-sessions/templelinux.desktop`).
  - `templelinux-session` starts the compositor (initially `sway`) + fullscreen `templeshell` reliably.
  - `TEMPLEOS_ROOT` discovery works when installed system-wide (default should be `/usr/share/templelinux/TempleOS`).

Notes / constraints:
- If bundling TempleOS sources/assets in binary packages is undesirable or legally constrained, split packaging into:
  - `templelinux` (binaries + scripts + session files), and
  - `templelinux-templeos-data` (TempleOS tree),
  - or add an explicit first-run “download TempleOS tree” step (with user consent) and cache it under `/usr/share/templelinux/` or `~/.templelinux/`.

Progress notes (2026-02-17):
- Added Arch AUR packaging skeleton under `packaging/arch/templelinux-git/`:
  - `PKGBUILD` installs TempleLinux binaries, `templelinux-session`, and the Wayland session entry.
  - Installs the vendored TempleOS tree to `/usr/share/templelinux/TempleOS` for out-of-the-box `TEMPLEOS_ROOT` discovery.
- Added Debian/Ubuntu packaging scripts under `packaging/debian/`:
  - `build-debs.sh` builds two packages: `templelinux` and `templelinux-templeos-data`.
  - Includes `.gitignore` so local build artifacts don’t pollute the repo.
- Expanded `README.md` install instructions (from-source, Arch, Debian/Ubuntu, sway session, uninstall).
- Tweaked the Arch `PKGBUILD` to disable debug subpackages (`options=('!debug')`) since the TempleOS tree includes non-debug binaries and makes `makepkg` emit noisy `gdb-add-index` errors.
- Fixed `packaging/debian/build-debs.sh` version parsing and validated `.deb` builds in an Ubuntu 24.04 Docker container (artifacts in `packaging/debian/dist/`).
- Tweaked Debian runtime deps to prefer real ALSA (`libasound2t64 | libasound2`) so `apt install` won’t accidentally satisfy `libasound2` via OSS shim packages that break audio symbol resolution.

---

## 11) Practical build & packaging notes (Linux)

### System-wide install layout (for distro packages)

For a real “install on top of Arch/Ubuntu” experience, packages should install:

- `/usr/bin/templeshell` (+ other `temple-*` bins)
- `/usr/bin/templelinux-session` (starts the compositor + TempleShell on workspace 1)
- `/usr/share/wayland-sessions/templelinux.desktop` (so GDM/SDDM can offer a “TempleLinux” session)
- `/usr/share/templelinux/TempleOS/` (TempleOS sources/assets; enables automatic `TEMPLEOS_ROOT` discovery)

Per-user writable state stays under `~/.templelinux/` (created on first run).

### Specification: TempleLinux Startup and Session Management (v1.0)

Status: Intended behavior (source-of-truth for startup/session UX; implement incrementally).

Component scope:
- `templeshell` (core binary)
- `packaging/bin/templelinux-session` (integration script / session launcher)

#### 1) Overview

TempleLinux supports two launch modes:

1) **Standard Application Mode (default)**: `templeshell` runs as a graphical client inside an existing desktop session (GNOME/KDE/i3/etc).
2) **Dedicated Session Mode (sway)**: `templelinux-session` starts a specialized Wayland session where `templeshell` “owns” workspace 1 and Linux apps live on workspace 2.

#### 2) Environment & asset discovery phase (must run before graphics init)

##### 2.1 Root directory initialization

- Read `TEMPLE_ROOT`.
- Default: `$HOME/.templelinux` when unset.
- Ensure the directory exists and contains (create if missing):
  - `Home/` (user files)
  - `Doc/` (documentation overlays)
  - `Cfg/` (configuration, history, variables)
  - `Apps/` (local HolyC apps)

##### 2.2 Upstream TempleOS discovery

- Read `TEMPLEOS_ROOT`.
- If unset: traverse parents searching for `third_party/TempleOS/Kernel/FontStd.HC`.
- System fallback: `/usr/share/templelinux/TempleOS`.
- Fail fast: if the TempleOS tree cannot be found/validated (at minimum `Kernel/FontStd.HC` and `Adam/Gr/GrPalette.HC`), abort immediately with a clear error explaining how to fix it (submodule missing vs. env var).

#### 3) Standard Application Mode (windowed/fullscreen)

- Window creation:
  - Use `winit` to create a surface; select Wayland/X11 automatically.
  - Default to fullscreen (overrideable via `--no-fullscreen`).
  - Internal resolution is fixed at `640×480`; output resolution matches the current monitor/window.
- Scaling strategy:
  - Nearest-neighbor (`wgpu` sampler: `Nearest`).
  - Maintain 4:3 aspect ratio; letterbox with black bars when needed.
- Input focus:
  - On focus loss: enter a “paused” state (ignore input; throttle rendering).
  - On focus regain: flush queued input and restore the prior state.

#### 4) Dedicated Session Mode (sway integration)

- `templelinux-session` must generate a sway config that:
  - Defines workspace 1 (`"1:Temple"`) for `templeshell`:
    - assigns `templeshell` to it,
    - forces fullscreen,
    - hides bars/overlays on that workspace for a clean slate.
  - Defines workspace 2 (`"2:Linux"`) as the default destination for non-Temple windows.
- Launch behavior:
  1) User runs `templelinux-session` (from TTY or a display manager session entry).
  2) Script starts `sway` with the generated config.
  3) `templeshell` starts on workspace 1.
- Workspace bridging:
  - Host integration commands (`browse`, `open`, `run`) in TempleShell should switch to workspace 2 (via `swaymsg`) before spawning the external Linux process, so the user sees the launched app immediately.
  - Users return to workspace 1 manually (or via hotkey like `Super+1`).

#### 5) Runtime initialization sequence (after graphics init)

1) IPC server setup: bind Unix socket at `TEMPLE_SOCK`.
   - Default socket path: `$XDG_RUNTIME_DIR/temple.sock` or `$TEMPLE_ROOT/temple.sock` if runtime dir is unavailable.
2) Start async listener thread for clients (`temple-hc`, `temple-edit`, etc.).
3) AutoRun: if `$TEMPLE_ROOT/Cfg/AutoStart.tl` exists, execute commands line-by-line.
4) Initial render: draw prompt background and command line.
5) Enter the `winit` event loop (60 FPS target; throttle when idle/paused).

#### 6) Graceful shutdown

##### 6.1 User-initiated

- Triggers: shell command `exit`/`bye`, or compositor window close.
- Behavior:
  - Send `MSG_SHUTDOWN` to all connected IPC clients.
  - Wait up to ~100ms for clients to acknowledge/disconnect.
  - Persist shell history and variables.
  - Remove the IPC socket file.
  - Exit with code `0`.

##### 6.2 Crash / signal handling

- Triggers: `SIGTERM`, `SIGINT`, or panic.
- Behavior:
  - Close window and IPC socket promptly.
  - Best-effort logging to `stderr` or `$TEMPLE_ROOT/Cfg/CrashLog.txt`.

#### 7) Launch arguments

| Argument | Type | Description |
| :--- | :--- | :--- |
| `--no-fullscreen` | Flag | Override default; start in a resizable window (640×480 minimum) |
| `--config <path>` | Path | Explicitly set `TEMPLE_ROOT` |
| `--os-root <path>` | Path | Explicitly set `TEMPLEOS_ROOT` |
| `--sock <path>` | Path | Explicitly set IPC socket path |

### Session startup (typical)

Provide a `.desktop` session entry or systemd user service that starts:

1) compositor
2) TempleShell on workspace 1 (full-screen)
3) optional: pre-launch a Linux workspace (workspace 2) setup (terminal, browser, etc.)
- In this repo (sway example):
  - `packaging/wayland-sessions/templelinux.desktop` (copy to `/usr/share/wayland-sessions/`)
  - `packaging/bin/templelinux-session` (copy to `/usr/local/bin/` and `chmod +x`)
- Install binaries for the session:
  - `cargo install --path .` (installs `templeshell` + `temple-*` bins into `~/.cargo/bin`)

### X11 apps

- Ensure XWayland support is present in the compositor for legacy apps.
- The sample `templelinux-session` sway config enables XWayland via `xwayland enable`.

### Automated testing (headless X11/Wayland)

We have local virtual framebuffer support for both X11 and Wayland (e.g. Xvfb + weston headless, or a local wrapper like `fvxb`). Use it to test TempleShell without a physical display:

- Fast, no-GUI correctness: `cargo test` (includes protocol-level upstream demo tests that don’t require a window).
- Planned GUI regression harness (Milestone 38):
  - X11: `xvfb-run -a …` to run TempleShell + dump a screenshot.
  - Wayland: `weston --backend=headless-backend.so …` to run TempleShell + dump a screenshot.

### “Back to Temple” UX

- Compositor hotkey is simplest and most reliable.
- The sample sway config binds `Super+1` → workspace 1 (Temple) and `Super+2` → workspace 2 (Linux).
- TempleShell’s status line shows: `WS: Super+1 Temple  Super+2 Linux`.
- TempleShell also provides `ws <temple|linux|num>` (sway only). Set `TEMPLE_AUTO_LINUX_WS=1` to auto-switch to the Linux workspace after `open`/`browse`.

---

## 12) Open decisions (make these early)

- Compositor target: `sway` vs `weston` vs other (examples currently target `sway`).
- Primary TempleOS compatibility strategy:
  - Chosen: **run TempleOS apps from source** using a HolyC runtime + a TempleOS API compatibility layer (no guest OS).
- HolyC execution strategy:
  - Pending: interpreter/bytecode engine only vs hybrid with AOT compilation.
- App isolation strategy:
  - Pending: run HolyC apps in-process vs out-of-process (crash containment vs performance).
- TempleShell implementation stack:
  - Chosen: Rust (`winit` + `wgpu`) (already implemented).
- Visual assets:
  - Chosen: **use TempleOS assets** as the source of truth (font + palette, and any other UI resources we can extract).
- Internal resolution strategy:
  - Chosen: default to **640×480** (TempleOS native). Optional additional internal sizes can come later as a “bigger Temple” mode (still pixel-perfect).
- Scaling default:
  - Chosen: integer+letterbox (implemented); stretch-to-fill remains optional.
- Native “Temple-like” apps:
  - Optional: keep `temple-rt` for native tools and experiments; the primary compatibility target is HolyC apps from `third_party/TempleOS`.

---

## 13) Risk register (things that can bite)

- **Compositor scaling**: if you accidentally rely on compositor scaling, you may get linear filtering.
- **Fractional scaling / HiDPI**: test on fractional scale factors; keep mapping correct.
- **Input focus**: ensure workspace switching and focus behaves reliably (TempleShell vs Linux apps).
- **HolyC compatibility**: HolyC is a nonstandard dialect; TempleOS sources use unique constructs (`#exe`, special types, inline asm, etc.).
- **TempleOS API surface area**: many apps assume a big “OS” library; we must choose a compatibility target and grow it deliberately.
- **Safety/sandboxing**: running untrusted HolyC code in-process is dangerous; consider process isolation and capability restrictions.
- **Performance**: a pure interpreter may be slow for heavy demos/apps; hybrid/AOT may become necessary.
- **Licensing/redistribution**: confirm the licensing terms for bundling TempleOS source/assets; avoid accidental redistribution issues.

---

## 14) TempleOS similarity targets (what “feels like TempleOS”)

This project is not running the TempleOS kernel; instead, it runs TempleOS apps (HolyC) on Linux.
So the similarity target is:

- The **Temple workspace** (TempleShell) should look and feel like TempleOS.
- We should use **TempleOS assets and conventions** wherever possible, and avoid “modern UI” cues.

### 14.1 Visual + interaction fidelity

- Use TempleOS’ classic **640×480** look (16-color, 8×8 font) by default.
- Scale nearest-neighbor only; integer scale preferred; letterbox rather than blur.
- Keep input semantics “TempleOS-ish” (simple keyboard + mouse; avoid heavy IME/complex composition in the core apps).
- Render with TempleOS assets:
  - `sys_font_std` (font),
  - `gr_palette_std` (palette),
  - TempleOS border/line-drawing conventions.

### 14.2 Workflow fidelity (the big one)

- The Temple workflow should be available *inside TempleShell*:
  - editor (Adam-like),
  - DolDoc/`.DD` documentation browser,
  - run HolyC programs, see errors, jump to source, repeat.
- Make modern tasks one keystroke away:
  - `Super+2` → Linux workspace (Chrome, editors, etc.),
  - `Super+1` → back to TempleShell.
- Provide a TempleOS-style “LinuxBridge” app (HolyC) so you can launch Linux apps/URLs without leaving the Temple UI.

### 14.3 Compatibility mindset

- Prefer running original TempleOS HolyC sources from `third_party/TempleOS/` over rewriting them in Rust.
- Treat `third_party/TempleOS` as the canonical source/docs tree and make it runnable (Milestones 31–35).
- Only build native (`temple-rt`) apps when they are host-integration tools or when HolyC compatibility is not worth the effort.
- Keep Linux apps as Linux apps (browser/editor/etc.); integration is via workspace separation + explicit bridges.

---

## 15) Proposed “Temple root” directory layout (TempleOS-ish)

We need a consistent “Temple filesystem” view that:

- matches TempleOS path conventions enough for apps/docs to load,
- keeps vendored TempleOS sources read-only,
- provides a writable user area for projects and settings.

### 15.1 Virtual path conventions (compat)

Targets:
- `::/` → `third_party/TempleOS/` (read-only)
- `/` → `~/.templelinux/` (writable)

Notes:
- Many TempleOS sources refer to `::/Doc/...` and `::/Demo/...`. We should support those paths directly.

### 15.2 Host directory layout

Under `~/.templelinux/`:

- `Home/` — user files
- `Cfg/` — TempleShell + HolyC runtime config
- `Tmp/` — scratch
- `Ports/` — local ports/notes/diffs
- `Cache/` — compiled artifacts / bytecode cache (if used)
- `Logs/` — crash logs / runtime logs

Notes:
- Keep “real Linux” reachable, but not the default. Linux apps live on the Linux workspace.

---

## 16) App model: TempleShell desktop

TempleShell is the “desktop”:

- It provides the fixed-resolution pixel world (640×480 default).
- It hosts a HolyC runtime and a TempleOS API layer so TempleOS apps can run.
- It provides windowing/multi-app UX as needed (start single-app, grow to multi-window).

Linux apps remain normal Linux apps on the Linux workspace.

---

## 17) Stretch goals (only after the core feels right)

- Multiple internal resolutions beyond TempleOS 640×480 (e.g. 800×600 host mode, 1024×768 “bigger Temple” mode).
- Hardware cursor + software cursor toggle.
- Simple sprite/icon pack for a more “desktop” feel.
- Recording: capture the internal framebuffer to a GIF/MP4 for sharing.
- A tiny “demo scene” mode: palette cycling, raster bars, plasma, etc.
- Wild but possible: embed Linux app windows *into* the Temple workspace via capture + composition (so Linux apps can appear “inside Temple”).

---

## 18) Docs strategy (reuse DolDoc / `.DD` as much as possible)

TempleOS already has DolDoc and a large `.DD` doc corpus. “Reuse first” means:

- Prefer using TempleOS’ own doc content and conventions:
  - ideally by running the original docs viewer code from `third_party/TempleOS` under our HolyC runtime,
  - otherwise by parsing/rendering **TempleOS `.DD`** directly (subset-first).

The goal is the same workflow:

- docs live in the tree,
- docs are easy to write,
- docs are easy to navigate (keyboard + mouse),
- docs can link to source and runnable examples.

### 18.1 File format options

- **Option A (recommended): render TempleOS `.DD` directly (subset first)**
  - Implement a host-side parser/renderer for the subset needed by TempleShell/bridges.
  - Start with: plain text, colors, links, simple headings, inline code.
  - Expand only when a real doc requires it.

- **Option B: convert `.DD` → a simpler intermediate format**
  - A conversion tool preprocesses `.DD` into a compact representation used by TempleShell.
  - Keep `.DD` as the source of truth so content stays TempleOS-authentic.

- **Option C (fallback): Markdown for host-only docs**
  - If we need host-only docs that aren’t present in TempleOS, allow Markdown,
    but still render with TempleOS font/palette so it doesn’t feel “off”.

### 18.2 Navigation + linking

Targets to support:
- `doc:/Doc/Foo.DD` (open another TempleOS doc page)
- `file:/...` (open a file in a viewer/editor)
- `run:linux <cmd...>` (launch a Linux app/command on the Linux workspace)
- `run:temple <path>` (launch a TempleOS app from the vendored tree, if supported)

Nice-to-have:
- Anchors inside a doc: `doc:/Doc/Foo.DD#SectionName`

### 18.3 Rendering model

- Render docs into a scrollable “page” (not just a terminal log).
- Keep the rendering grid aligned to 8×8 cells for crispness.
- Keep link hitboxes aligned to cell bounds so mouse selection feels snappy.

---

## 19) HolyC compatibility target (incremental, port-driven)

TempleLinux’s core compatibility problem is: **execute TempleOS HolyC programs on Linux**.

Guiding rule:
- **TempleOS apps drive features**. Avoid “full compiler first”.

### 19.1 Compatibility phases

Phase 1 (already started):
- expressions, if/while, calls, integers/strings, a few graphics + input built-ins.

Phase 2 (ports start to get real):
- `for`, `switch`, `break`/`continue`
- arrays, pointers, address-of/deref
- structs/enums
- bitwise ops + shifts
- char literals + escape sequences
- TempleOS-isms needed by real source files (`#help_index`, basic macro patterns, etc.).

Phase 3 (developer experience):
- multi-file projects and includes
- better error messages (line/col with snippets)
- more stdlib helpers (strings, memory, math)
- basic debugger hooks (trace, watch variables) if feasible
- better build-time features (handling TempleOS `#exe` blocks where possible, or providing clear fallbacks).

Phase 4 (ambitious):
- a compiler path (transpile-to-C or LLVM)
- caching/AOT for frequently-used apps

### 19.2 HolyC runtime surface area (keep it small)

For similarity and portability:
- Prefer a small set of “syscalls” implemented in TempleLinux (backed by `temple-rt`) over re-creating big libc-style APIs.
- Keep graphics and input deterministic and frame-based (fits the fixed-resolution pixel world).

---

## 20) Candidate “system apps” (Temple vibe checklist)

TempleOS already comes with a large set of “system apps” (docs, editor, demos, games).
Because our goal is “run original apps + match visuals”, the priority is:

1) Make a representative set of original system apps runnable from `third_party/TempleOS/`.
2) Add only a small number of TempleLinux-specific apps for integration.

### 20.1 Original TempleOS apps (preferred)

Aim to run these from source (not ports):
- docs viewer (DolDoc/`.DD`)
- editor (Adam)
- file manager
- a few demos/games

Treat `third_party/TempleOS` as the canonical tree to browse/run/learn from (Milestones 31–35).

### 20.2 TempleLinux-specific HolyC apps

- **LinuxBridge**: launch URLs/files/commands on the Linux workspace (Milestone 36).
- Optional: **ClipboardBridge**: copy/paste plain text via the host clipboard.
- Optional: **FileBridge**: quick “copy to/from” host directories.

### 20.3 Host-side helpers

- TempleShell UI + app launcher.
- A small bridge helper that executes `xdg-open` / a command allowlist safely (optional).

---

## 21) TempleShell UX polish (TempleOS-ish ergonomics)

Quality-of-life items that dramatically improve “OS feel”:

- Tab completion for commands and paths (Temple root scoped).
- Rich `help` output: list commands, show usage, link to docs.
- Consistent keybindings (Esc to exit apps, F1 help, etc.).
- A visible status line: time, active app/window, workspace hint, last error.
- A small “notifications” log inside TempleShell for app crashes and IPC errors.

---

## 22) Linux integration philosophy (to stay TempleOS-ish)

TempleOS intentionally avoided network complexity; TempleLinux can keep that spirit:

- Temple apps default to a “closed world” (no network APIs by default).
- Launch networked things via Linux apps (`browse`, `open`, `run`) on the Linux workspace.
- If network APIs are added for Temple apps later, keep them explicit and opt-in (so the default vibe stays “offline OS”).

---

## 23) Codebase health (avoid monoliths / keep it refactorable)

We’re intentionally shipping a lot of functionality quickly, but we still need to keep the host-side code modular so compatibility work doesn’t turn into “edit one 8k-line file forever”.

Targets:
- Keep Rust files **< ~1,000 LOC** where practical (split into modules; prefer composition over mega-files).
- Keep `third_party/TempleOS/` as an upstream mirror (read-only) and **do not fork** unless unavoidable.

Planned refactors (no behavior changes):
- Split `src/bin/temple_hc.rs` into modules:
  - `lexer` / `parser` / `ast`
  - `preprocess` (includes + defines)
  - `vm` (execution + heap + objects)
  - `builtins` (TempleOS API shims)
  - `fmt` (TempleOS-style formatting)
  - move the large `#[cfg(test)]` harness into a `tests` module file
- Split `src/main.rs` into modules:
  - terminal + command parsing
  - file browser
  - DolDoc viewer
  - app/window manager
  - rendering + screenshot/test mode plumbing

Acceptance:
- `cargo test` passes unchanged.
- Golden-image tests (`TEMPLE_GUI_TESTS=1`) still pass.
- No user-facing behavior changes; only structure.

Progress notes (2026-02-05):
- Split the `temple-hc` monolith into smaller files under `src/bin/temple_hc/` and replaced `src/bin/temple_hc.rs` with `include!(...)` stubs (same semantics, easier to navigate).
- Split the `templeshell` monolith into smaller files under `src/templeshell/` and replaced `src/main.rs` with an `include!(...)` stub; fixed shader include path (`src/templeshell/03_gfx.rs` now `include_str!("../shader.wgsl")`).
- Made GUI golden tests stable:
  - `--test-run-shell` now runs stepwise (and waits for app connect after `tapp ...`) so multi-app screenshots are deterministic.
  - In test modes, ignore real keyboard/mouse input so headless runs don’t vary based on Xvfb pointer/key state.
  - Updated the `multiwindow` golden SHA in `tests/gui_smoke.rs`.
