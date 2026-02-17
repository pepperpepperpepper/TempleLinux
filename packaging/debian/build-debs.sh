#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"

if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "build-debs.sh: dpkg-deb not found (run this on Debian/Ubuntu, or install dpkg-deb)" >&2
  exit 127
fi

cd "${repo_root}"

arch="$(dpkg --print-architecture)"
short_sha="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
crate_ver="$(grep -m1 '^version[[:space:]]*=' Cargo.toml | sed -E 's/.*version[[:space:]]*=[[:space:]]*\"([^\"]+)\".*/\1/' || true)"
crate_ver="${crate_ver:-0.0.0}"
version="${crate_ver}+git${short_sha}"

echo "TempleLinux deb build:"
echo "  arch:    ${arch}"
echo "  version: ${version}"

echo "Building release binaries..."
cargo build --release --locked

work_dir="${script_dir}/work"
dist_dir="${script_dir}/dist"
main_stage="${work_dir}/templelinux"
data_stage="${work_dir}/templelinux-templeos-data"

if [[ -d "${work_dir}" ]]; then
  rm -r "${work_dir}"
fi
mkdir -p "${main_stage}" "${data_stage}" "${dist_dir}"

mkdir -p "${main_stage}/usr/bin"
install -m755 "target/release/templeshell" "${main_stage}/usr/bin/templeshell"
install -m755 "target/release/temple-demo" "${main_stage}/usr/bin/temple-demo"
install -m755 "target/release/temple-hc" "${main_stage}/usr/bin/temple-hc"
install -m755 "target/release/temple-paint" "${main_stage}/usr/bin/temple-paint"
install -m755 "target/release/temple-edit" "${main_stage}/usr/bin/temple-edit"
install -m755 "packaging/bin/templelinux-session" "${main_stage}/usr/bin/templelinux-session"

mkdir -p "${main_stage}/usr/share/wayland-sessions"
install -m644 "packaging/wayland-sessions/templelinux.desktop" \
  "${main_stage}/usr/share/wayland-sessions/templelinux.desktop"

mkdir -p "${data_stage}/usr/share/templelinux/TempleOS"
tar --exclude=.git -C "third_party/TempleOS" -cf - . \
  | tar -C "${data_stage}/usr/share/templelinux/TempleOS" -xf -

mkdir -p "${main_stage}/DEBIAN" "${data_stage}/DEBIAN"

cat >"${main_stage}/DEBIAN/control" <<EOF
Package: templelinux
Version: ${version}
Section: misc
Priority: optional
Architecture: ${arch}
Depends: xdg-utils, libasound2t64 | libasound2, libx11-6, libxcb1, libwayland-client0, libxkbcommon0, libvulkan1
Recommends: sway, xwayland
Maintainer: TempleLinux contributors
Description: TempleOS-inspired environment on Linux (TempleShell + HolyC runtime)
 TempleLinux is a Linux-native runtime + UI environment inspired by TempleOS.
 It provides a full-screen "Temple" UI (templeshell) plus a HolyC runtime (temple-hc)
 capable of running representative, unmodified upstream TempleOS demos/apps.
EOF

cat >"${data_stage}/DEBIAN/control" <<EOF
Package: templelinux-templeos-data
Version: ${version}
Section: misc
Priority: optional
Architecture: all
Depends: templelinux (= ${version})
Maintainer: TempleLinux contributors
Description: TempleOS source/assets tree for TempleLinux
 This package installs the vendored upstream TempleOS sources/assets under:
   /usr/share/templelinux/TempleOS
 This enables TEMPLEOS_ROOT auto-discovery so TempleLinux can run upstream demos/apps.
EOF

main_deb="${dist_dir}/templelinux_${version}_${arch}.deb"
data_deb="${dist_dir}/templelinux-templeos-data_${version}_all.deb"

echo "Building debs:"
echo "  ${main_deb}"
echo "  ${data_deb}"

dpkg-deb --root-owner-group --build "${main_stage}" "${main_deb}"
dpkg-deb --root-owner-group --build "${data_stage}" "${data_deb}"

echo "Done."
