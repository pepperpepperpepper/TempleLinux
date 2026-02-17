# TempleLinux — Project Description / Analysis

Last updated: 2026-02-17

## Summary

TempleLinux is a Linux-native, TempleOS-inspired environment that aims to reproduce the **TempleOS “feel”** (fixed-resolution framebuffer, 8×8 font, limited palette, DolDoc documents, sprite-heavy UI conventions) while still running on a normal Linux system with a standard compositor and normal Linux apps.

TempleLinux does **not** run the TempleOS kernel. Instead, it focuses on **source-level compatibility**:

- It runs TempleOS programs from their **HolyC source** (primarily from the vendored TempleOS tree).
- It implements a practical subset of the **TempleOS API surface** that those programs expect.
- It renders the result in a TempleOS-style UI host (`templeshell`) with a fixed internal framebuffer (currently **640×480**) and **nearest-neighbor** scaling.

## What it is

- A full-screen (or windowed) graphical shell: `templeshell`
  - Draws a fixed-size internal framebuffer and scales it to your monitor/window.
  - Provides a TempleOS-like prompt UI, doc viewer, and “PersonalMenu” icon desktop.
  - Hosts a small “Temple app” windowing model (multiple Temple apps can be shown at once).
- A HolyC runtime: `temple-hc`
  - Loads and executes a useful subset of HolyC as used by real TempleOS demos/apps.
  - Implements TempleOS-ish built-ins for graphics/input/files/docs and a small host-bridge for launching Linux apps.
- A runtime/protocol layer: `temple_rt`
  - Defines an IPC protocol for Temple apps.
  - Provides a “device runtime” used by both `temple-hc` and Rust demo apps to draw/present frames and receive input.
- A vendored upstream TempleOS tree (git submodule): `third_party/TempleOS/`
  - Used as the canonical source corpus for apps/docs/assets.
  - Used at build time to extract real TempleOS assets (font + palette).

## What it is not

- Not the TempleOS kernel, boot process, or scheduler.
- Not a VM image or machine emulator.
- Not a binary compatibility layer for TempleOS ISOs/binaries.
- Not a “theme pack” that skins a Linux terminal.

## How it works (high-level)

### Rendering model (TempleOS-like pixels on a modern desktop)

- `templeshell` maintains an internal framebuffer of **640×480**.
- Rendering is done in “Temple pixels” (palette-indexed / cell-based UI primitives).
- Output is scaled with nearest-neighbor and letterboxed to preserve the **4:3** aspect ratio.

This is the core trick that makes the output look like TempleOS rather than “a modern app pretending”.

### Temple apps and IPC

Temple apps (HolyC programs via `temple-hc`, or Rust demo apps) connect to `templeshell` over a Unix-domain socket:

- `templeshell` acts as the **host**:
  - Accepts connections.
  - Allocates a shared-memory framebuffer for each app (`memfd` + FD passing).
  - Forwards keyboard/mouse input to the focused app.
- Apps act as **clients**:
  - Draw into the shared framebuffer.
  - Send “Present()” style messages.

This keeps the core UI in `templeshell` while allowing Temple apps to be separate processes.

### HolyC execution strategy

TempleLinux’s HolyC strategy is source-level:

- Programs are loaded from the TempleOS tree (e.g. `::/Demo/.../*.HC`) or from the user’s writable Temple root (e.g. `/Home/...`).
- The HolyC runtime (`temple-hc`) interprets/executes a subset of HolyC sufficient to run a representative set of upstream TempleOS demos/apps.
- Where upstream programs assume a TempleOS environment, TempleLinux implements the needed “OS-like” functions as built-ins.

### Linux integration (“dual-OS” feel)

TempleLinux intentionally keeps Linux “real”:

- Inside TempleShell, commands like `browse`, `open`, and `run` can launch Linux apps via `xdg-open` / process spawning.
- In the dedicated sway session mode, Linux apps are intended to live on a separate workspace (workspace 2), while TempleShell stays full-screen on workspace 1.

## Data locations and discovery

- `TEMPLE_ROOT` (writable “Temple drive”)
  - Default: `$HOME/.templelinux`
  - Contains: `Home/`, `Doc/`, `Cfg/`, `Apps/`
- `TEMPLEOS_ROOT` (read-only upstream TempleOS tree)
  - Auto-discovery:
    - search for `third_party/TempleOS/Kernel/FontStd.HC` near the repo/binary, or
    - `/usr/share/templelinux/TempleOS` (system-wide install)

## Entry points and scripts

- Core binaries:
  - `templeshell` — the graphical host shell
  - `temple-hc` — HolyC runtime
  - `temple-edit` — Adam-like editor workflow
  - `temple-paint`, `temple-demo` — demo Temple apps
- Session integration:
  - `packaging/bin/templelinux-session` — starts a sway session with TempleShell on workspace 1
  - `packaging/wayland-sessions/templelinux.desktop` — optional display-manager session entry

## Compatibility + regression strategy

TempleLinux treats the upstream TempleOS tree as the “test suite”:

- Fast protocol-level tests: `cargo test -q`
- GUI golden tests (headless Xvfb): `TEMPLE_GUI_TESTS=1 cargo test -q --test gui_smoke`
- Screenshot gallery capture: `packaging/bin/templelinux-publish-screenshots`

