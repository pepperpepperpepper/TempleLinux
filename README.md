# TempleLinux

TempleLinux is a TempleOS-inspired environment for Linux: a full-screen “Temple” UI that aims for TempleOS-like visuals (palette, font, DolDoc, sprites) while running on a normal Linux desktop.

This repo vendors upstream TempleOS sources under `third_party/TempleOS/` and focuses on running representative, unmodified HolyC programs inside `templeshell`.

If you cloned without submodules, run:

```bash
git submodule update --init --recursive
```

## What is TempleLinux? (deep overview)

TempleLinux is a Linux-native software stack that tries to recreate the **TempleOS “feel”** (palette, 8×8 font, DolDoc documents, sprite-heavy UI conventions, lightweight apps) **without running the TempleOS kernel**.

Instead of virtualizing or emulating the operating system, TempleLinux focuses on **source-level compatibility**:

- TempleOS programs are executed from their **HolyC source** (primarily from the vendored TempleOS tree).
- A small, practical subset of the TempleOS API is implemented and mapped onto a Linux-hosted runtime.
- A full-screen graphical shell (“TempleShell”) renders to a **fixed internal framebuffer** (currently **640×480**) and scales it with **nearest-neighbor** to keep pixels crisp.
- Linux remains “real” underneath: the compositor, drivers, packages, browser, editor, etc. are normal Linux programs—optionally living on a separate workspace.

This repo’s scope is explicitly “TempleOS apps + aesthetics on Linux”, not “TempleOS as an OS”.

### What this project is (and isn’t)

TempleLinux is best thought of as a **TempleOS-flavored runtime + UI environment** hosted by Linux:

- **A graphical shell** (`templeshell`) that draws a TempleOS-like screen and hosts a minimal “Temple app” windowing model.
- **A HolyC runtime** (`temple-hc`) that can compile and execute a useful subset of HolyC used by real TempleOS demos/apps.
- **A compatibility layer** that implements a subset of TempleOS APIs (graphics, input/messages, docs, filesystem, sound).
- **A shared protocol + runtime** (`temple_rt`) used by HolyC and Rust apps to draw into a TempleShell-managed framebuffer via IPC.
- **A vendored upstream TempleOS source tree** (git submodule) used as the canonical “app and asset corpus” (`third_party/TempleOS/`).

TempleLinux is not:

- The TempleOS kernel or boot process.
- A full machine emulator or VM image.
- A binary compatibility layer for TempleOS binaries/ISOs.
- A “theme pack” that just skins a Linux terminal.

### How it works (high-level)

#### Rendering model: TempleOS-like pixels on a modern desktop

`templeshell` maintains a fixed-size internal framebuffer (**640×480**). All UI logic runs in that coordinate system and is scaled to your monitor/window with nearest-neighbor sampling. A 4:3 aspect ratio is preserved via letterboxing so the output isn’t distorted.

This “fixed internal buffer + nearest scaling” is the core trick that makes the output look like TempleOS rather than “a modern app pretending”.

#### Temple apps + IPC

Temple apps (HolyC programs via `temple-hc`, or native Rust demo apps) connect to `templeshell` over a Unix-domain socket (`TEMPLE_SOCK`).

- `templeshell` hosts the IPC server.
- Each app gets a shared-memory framebuffer (via `memfd` + FD passing).
- Keyboard/mouse input is forwarded to the focused app.
- Apps draw/present frames and receive events through the shared protocol (`temple_rt::protocol`).

This keeps the “Temple workspace” UI in one place while letting Temple apps run out-of-process.

#### HolyC execution strategy

The compatibility strategy is source-level:

- Programs are loaded from the TempleOS tree (e.g. `::/Demo/.../*.HC`) or from the user’s writable Temple root (e.g. `/Home/...`).
- `temple-hc` interprets/executes a subset of HolyC sufficient to run a representative set of upstream TempleOS demos/apps.
- Where upstream programs assume a TempleOS environment, TempleLinux implements the needed “OS-like” functions as built-ins and grows coverage based on real programs.

#### Linux integration (“dual-OS” feel)

TempleLinux keeps Linux “real”:

- Inside TempleShell, commands like `browse`, `open`, and `run` can launch Linux apps via `xdg-open` / process spawning.
- In the dedicated session mode, TempleShell is intended to stay full-screen on workspace 1 while Linux apps live on workspace 2 (see `packaging/bin/templelinux-session`).

### Where data lives

- `TEMPLE_ROOT` (writable “Temple drive”)
  - Default: `$HOME/.templelinux`
  - Contains: `Home/`, `Doc/`, `Cfg/`, `Apps/`
- `TEMPLEOS_ROOT` (read-only upstream TempleOS tree)
  - Auto-discovery:
    - a nearby `third_party/TempleOS/` (repo/submodule), or
    - `/usr/share/templelinux/TempleOS` (system-wide install)

More detail: `research.md`, `plan.md`, and `COMPATIBILITY.md`.

## Screenshots

<p>
  <img src="docs/screenshots/2026-02-17/personalmenu.png" width="320" alt="PersonalMenu icons">
  <img src="docs/screenshots/2026-02-17/doldoc-helpindex.png" width="320" alt="DolDoc HelpIndex">
</p>

<p>
  <img src="docs/screenshots/2026-02-17/multiwindow.png" width="320" alt="Multi-window (Paint + Demo)">
  <img src="docs/screenshots/2026-02-17/wallpaperfish.png" width="320" alt="WallpaperFish background">
</p>

More screenshots: `docs/screenshots/2026-02-17/`

## Install (experimental)

TempleLinux can be installed “on top of” an existing Linux distro (no custom ISO required).

There are two main ways to use it:

- **Run from a repo checkout** (best for development).
- **Install system-wide** using the packaging scaffolding under `packaging/` (best for “normal app” usage).

### Requirements (high level)

- A working graphics stack (Wayland or X11) with a Vulkan/OpenGL driver that works with `wgpu`.
- The upstream **TempleOS source/assets tree** must be available, either:
  - via the git submodule (`third_party/TempleOS/`), or
  - via a system install at `/usr/share/templelinux/TempleOS` (what the distro packages install).
- Optional (for the dedicated session): `sway` (+ `xwayland` if you want X11 apps in the “Linux” workspace).

### Option A: Run from source (any distro)

1) Clone with submodules:

```bash
git clone --recurse-submodules https://github.com/pepperpepperpepper/TempleLinux.git
cd TempleLinux
```

2) Build:

```bash
cargo build --release --locked
```

3) Run TempleShell from your current desktop session:

```bash
./target/release/templeshell
```

If you see errors about missing TempleOS files, ensure the submodule is present:

```bash
git submodule update --init --recursive
```

### Arch Linux (AUR-style `PKGBUILD`)

An AUR-style VCS package lives at `packaging/arch/templelinux-git/`.

```bash
sudo pacman -S --needed base-devel git
cd packaging/arch/templelinux-git
makepkg -si
```

This installs:

- TempleLinux binaries (`templeshell`, `temple-hc`, etc.) to `/usr/bin/`
- `templelinux-session` to `/usr/bin/`
- Wayland session entry to `/usr/share/wayland-sessions/templelinux.desktop`
- TempleOS sources/assets to `/usr/share/templelinux/TempleOS`

### Debian/Ubuntu (`.deb`)

Deb packaging scripts live in `packaging/debian/` and build **two** packages:

- `templelinux` (binaries + session integration)
- `templelinux-templeos-data` (TempleOS sources/assets under `/usr/share/templelinux/TempleOS`)

On Ubuntu/Debian:

```bash
sudo apt update
sudo apt install -y \
  dpkg-dev build-essential pkg-config git curl ca-certificates \
  libasound2-dev libx11-dev libxcb1-dev libwayland-dev libxkbcommon-dev \
  libvulkan-dev

# Install Rust (if you don't already have `cargo`):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

./packaging/debian/build-debs.sh
sudo apt install ./packaging/debian/dist/templelinux_*.deb ./packaging/debian/dist/templelinux-templeos-data_*.deb
```

### Dedicated session mode (Sway)

If you installed the packages above (or you have `templeshell` in `PATH`), you can start the dedicated session:

- From a **display manager** (GDM/SDDM): log out, then pick the **“TempleLinux”** Wayland session.
- From a **TTY**: run `templelinux-session`.

The dedicated session is currently implemented as a generated `sway` config:

- Workspace **1**: `templeshell` (fullscreen)
- Workspace **2**: Linux apps
- Hotkeys: `Super+1` (Temple), `Super+2` (Linux), `Super+Shift+E` (exit sway)

### Uninstall

- Arch: `sudo pacman -R templelinux-git`
- Debian/Ubuntu: `sudo apt remove templelinux templelinux-templeos-data`

### Troubleshooting

- “TempleOS tree not found”: ensure the submodule is checked out (`git submodule update --init --recursive`), install the `templelinux-templeos-data` package, or set `TEMPLEOS_ROOT` explicitly.
- Graphics/Vulkan issues: ensure your Vulkan driver stack is installed; if Vulkan is broken on your system, try forcing OpenGL: `WGPU_BACKEND=gl templeshell`.
- Dedicated session doesn’t start: `templelinux-session` requires `sway` to be installed and in `PATH`.

## Smoke tests

- Protocol-level: `cargo test -q`
- GUI goldens (headless/Xvfb): `TEMPLE_GUI_TESTS=1 cargo test -q --test gui_smoke`

See `COMPATIBILITY.md` for the current compatibility target + smoke suite definition.
