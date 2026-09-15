#!/usr/bin/env bash
# Tries the AppImage on clean distros in Docker, with nothing of Bloom's development setup:
#   1. every library inside it resolves, from the AppImage or a desktop's baseline, and
#   2. it starts on a virtual display, and after 20 seconds both Bloom and WebKit's page renderer
#      (WebKitWebProcess) are running. The renderer can die while the window stays open, so
#      Bloom alone running isn't enough.
# A clean container has no GPU, Discord or session bus, so this proves the package, not playback.
# Usage: packaging/test-appimage.sh [path/to/Bloom.AppImage]
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
image="$(realpath "${1:-$(ls "$root"/packaging/out/*.AppImage | head -1)}")"
echo "testing $image"

# The desktop baseline: the libraries AppImage's excludelist leaves to the system that Bloom's
# files need (worked out from the AppImage on 2026-09-15), with Mesa for EGL, OpenGL ES (which
# WebKit opens at run time, so it isn't in any file's needed list) and a virtual display.
# Nothing else, so a library the AppImage should carry and doesn't shows up as missing.
UBUNTU_BASELINE="xvfb xauth procps libasound2t64 libcom-err2 libdrm2 libegl1 libegl-mesa0 libgl1 libgles2 libgl1-mesa-dri
  libgbm1 libexpat1 libfontconfig1 libfreetype6 libfribidi0 libgmp10 libgpg-error0 libharfbuzz0b
  libpipewire-0.3-0t64 libstdc++6 libwayland-client0 libwayland-cursor0 libwayland-egl1 libwayland-server0 libx11-6 libx11-xcb1 libxcb-dri3-0 libxcb1 zlib1g"
FEDORA_BASELINE="xorg-x11-server-Xvfb xorg-x11-xauth which procps-ng alsa-lib libcom_err libdrm libglvnd-egl
  mesa-libEGL libglvnd-glx libglvnd-gles mesa-dri-drivers mesa-libgbm expat fontconfig freetype fribidi gmp libgpg-error
  harfbuzz pipewire-libs libstdc++ libwayland-client libwayland-cursor libwayland-egl libwayland-server libX11 libX11-xcb libxcb zlib-ng-compat
  libva"
# libva is only in Fedora's: Ubuntu's run takes the AppImage's fallback copy, Fedora's the system's.

for distro in ubuntu:24.04 fedora:42; do
  echo "== $distro"
  docker run --rm --shm-size=512m -v "$image":/Bloom.AppImage:ro \
    -e UBUNTU_BASELINE="$UBUNTU_BASELINE" -e FEDORA_BASELINE="$FEDORA_BASELINE" "$distro" bash -c '
    set -uo pipefail
    if command -v apt-get >/dev/null; then
      apt-get update -qq >/dev/null && apt-get install -y -qq --no-install-recommends $UBUNTU_BASELINE >/dev/null 2>&1
    else
      dnf install -y -q $FEDORA_BASELINE >/dev/null 2>&1
    fi
    cd /tmp && /Bloom.AppImage --appimage-extract >/dev/null
    # One line per thing missing (a library, or a glibc version), however many files need it.
    missing=$(find squashfs-root/usr/bin squashfs-root/usr/lib -type f \( -name "*.so*" -o -perm -u+x \) \
      -exec env LD_LIBRARY_PATH=squashfs-root/usr/lib:squashfs-root/usr/lib/libva-fallback ldd {} \; 2>&1 \
      | grep -oE "version .GLIBC_[0-9.]+. not found|[^[:space:]]+ => not found|[^[:space:]]+: cannot open shared object file" \
      | sort | uniq -c | sort -rn)
    if [ -n "$missing" ]; then echo "missing (count of files needing it):"; echo "$missing"; else echo "libraries: all resolve"; fi
    # Wayland'"'"'s libraries must come from the system, or the system'"'"'s Mesa EGL can'"'"'t load.
    wayland=$(find squashfs-root -name "libwayland-*" -printf "%f ")
    if [ -n "$wayland" ]; then echo "bundled Wayland libraries (must not be): $wayland"; else echo "bundled Wayland libraries: none"; fi
    # libva only as a fallback, all four of it, and the hook choosing between it and the system'"'"'s.
    loose=$(find squashfs-root/usr/lib -maxdepth 1 -name "libva*.so*" -printf "%f ")
    fallback=$(find squashfs-root/usr/lib/libva-fallback -name "libva*.so.2" 2>/dev/null | wc -l)
    if [ -n "$loose" ]; then echo "libva: FAILED, bundled where it would be used first: $loose"
    elif [ "$fallback" -ne 4 ]; then echo "libva: FAILED, $fallback of 4 libraries in libva-fallback"
    elif ! grep -q "libva-fallback" squashfs-root/apprun-hooks/linuxdeploy-plugin-gtk.sh; then echo "libva: FAILED, the startup hook never uses the fallback"
    else echo "libva: the system'"'"'s when it has it, the AppImage'"'"'s otherwise"; fi
    # Licence texts for what it bundles (THIRD_PARTY_NOTICES.md): mpv'"'"'s and FFmpeg'"'"'s at least.
    notices=$(find squashfs-root/usr/share/doc -name copyright | wc -l)
    if [ ! -f squashfs-root/usr/share/doc/mpv/LICENSE.GPL ]; then echo "licences: FAILED, no mpv licence"
    elif ! ls -d squashfs-root/usr/share/doc/libavcodec* >/dev/null 2>&1; then echo "licences: FAILED, no FFmpeg copyright"
    else echo "licences: $notices package copyright files, mpv and FFmpeg included"; fi
    # The startup hook must not force X11, and must turn Ubuntu'"'"'s NVIDIA patch off on Wayland.
    hook=squashfs-root/apprun-hooks/linuxdeploy-plugin-gtk.sh
    if grep -q "^export GDK_BACKEND=" "$hook"; then echo "startup hook: FAILED, still forces GDK_BACKEND"
    elif ! grep -q "WEBKIT_FORCE_DMABUF_RENDERER" "$hook"; then echo "startup hook: FAILED, no WEBKIT_FORCE_DMABUF_RENDERER"
    elif ! grep -q "WEBKIT_DMABUF_RENDERER_FORCE_SHM" "$hook"; then echo "startup hook: FAILED, no shared-memory buffers for X11 with NVIDIA"
    else echo "startup hook: no forced backend, NVIDIA patch overridden (Wayland; X11 with shared memory)"; fi
    # And must let the desktop'"'"'s look through: its GTK theme, KDE'"'"'s GTK modules, its cursor.
    if ! grep -q "unset GTK_THEME" "$hook"; then echo "desktop look: FAILED, Adwaita still forced"
    elif ! grep -q "/usr/lib/gtk-3.0" "$hook"; then echo "desktop look: FAILED, no Arch GTK module folder"
    elif [ ! -f squashfs-root/usr/lib/x86_64-linux-gnu/gio/modules/libdconfsettings.so ]; then echo "desktop look: FAILED, no dconf GSettings backend"
    else echo "desktop look: desktop theme, GTK modules and dconf backend"; fi
    (xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1 /Bloom.AppImage >run.log 2>&1 &)
    sleep 20
    app=$(pgrep -x bloom >/dev/null && echo running || echo gone)
    # By process name (the kernel cuts it to 15 characters): -f would match this script'"'"'s own text.
    renderer=$(pgrep -x WebKitWebProces >/dev/null && echo running || echo gone)
    if [ "$app" = running ] && [ "$renderer" = running ]; then echo "start: OK, Bloom and its page renderer running after 20 s"
    else echo "start: FAILED, Bloom $app, page renderer $renderer after 20 s"; fi
    used=$(grep -oE "/[^ ]*/libva\.so\.2[^ ]*" /proc/$(pgrep -x bloom | head -1)/maps 2>/dev/null | head -1)
    echo "libva loaded: ${used:-none}"
    grep -v "^$" run.log | tail -10
  '
done
