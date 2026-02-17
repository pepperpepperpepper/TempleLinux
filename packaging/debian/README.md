# Debian/Ubuntu packaging (experimental)

This folder contains a simple packaging script that builds **two** `.deb` files:

- `templelinux` — binaries + session integration (Wayland session entry, `templelinux-session`)
- `templelinux-templeos-data` — the vendored TempleOS source/assets tree installed under `/usr/share/templelinux/TempleOS`

This is intended to support “install on top of Ubuntu/Debian” without requiring a custom distro image.

## Build on Ubuntu

1) Install build deps (rough baseline; adjust for your distro):

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config git curl ca-certificates \
  libasound2-dev libx11-dev libxcb1-dev libwayland-dev libxkbcommon-dev \
  libvulkan-dev
```

2) Install Rust (if needed):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

3) Build:

```bash
./packaging/debian/build-debs.sh
```

Artifacts are written to `packaging/debian/dist/`.

## Install

```bash
sudo apt install ./packaging/debian/dist/templelinux_*.deb ./packaging/debian/dist/templelinux-templeos-data_*.deb
```

Then:

- Run `templeshell` from your existing desktop session, or
- pick “TempleLinux” in your display manager (Wayland sessions) to start the dedicated sway-based session.

