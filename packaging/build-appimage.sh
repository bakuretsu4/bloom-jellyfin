#!/usr/bin/env bash
# Builds Bloom's AppImage in the Ubuntu 24.04 builder (see Dockerfile) from the last commit, and
# puts it in packaging/out/. Uncommitted changes aren't included, so what's built is what's in git.
# Needs Docker. Caches (cargo registry, build target, Tauri's tools) live in Docker volumes, so
# later builds are much faster.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/packaging/out"
mkdir -p "$out"

docker build -t bloom-appimage-builder "$root/packaging"

git -C "$root" archive --format=tar HEAD | docker run --rm -i \
  -v bloom-cargo-registry:/opt/cargo/registry \
  -v bloom-target:/build/app/src-tauri/target \
  -v bloom-tauri-cache:/root/.cache/tauri \
  -v "$out":/out \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  bloom-appimage-builder bash -c '
    set -euo pipefail
    tar -x -C /build
    cd /build/app
    npm ci
    npm run tauri build
    built=$(ls /build/app/src-tauri/target/release/bundle/appimage/*.AppImage)

    # Take Wayland'"'"'s libraries back out. linuxdeploy bundles Ubuntu 24.04'"'"'s (1.22), and the
    # system'"'"'s graphics drivers need the system'"'"'s: Mesa'"'"'s EGL fails to load against 1.22
    # ("undefined symbol: wl_fixes_interface"), WebKit then finds no usable EGL, and Bloom
    # crashes the first time a page needs GPU compositing (opening a title). Tauri runs
    # linuxdeploy with its own arguments, so this repacks the finished AppImage, with its own
    # runtime and the appimagetool that comes with linuxdeploy'"'"'s AppImage plugin.
    rm -rf /tmp/tool /tmp/repack && mkdir -p /tmp/tool /tmp/repack
    (cd /tmp/tool && /root/.cache/tauri/linuxdeploy-plugin-appimage.AppImage --appimage-extract >/dev/null)
    cd /tmp/repack
    "$built" --appimage-extract >/dev/null
    find squashfs-root -name "libwayland-*" -printf "removed %f\n" -delete
    # libva the same way, kept as a fallback. Ubuntu'"'"'s looks for drivers in its own folder
    # (/usr/lib/x86_64-linux-gnu/dri, not Arch'"'"'s /usr/lib/dri or Fedora'"'"'s /usr/lib64/dri) and
    # can'"'"'t load a newer system driver anyway: on an Arch laptop with Intel graphics vaInitialize
    # failed and every file decoded in software. The startup hook uses the system'"'"'s libva when
    # it has one, and this folder only when it doesn'"'"'t (see packaging/apprun-bloom.sh).
    mkdir -p squashfs-root/usr/lib/libva-fallback
    find squashfs-root/usr/lib -maxdepth 1 -name "libva*.so*" -printf "moved %f to libva-fallback\n" \
      -exec mv {} squashfs-root/usr/lib/libva-fallback/ \;
    # The GTK hook forces X11 and Adwaita. Take out the X11 line, and add Bloom'"'"'s part at the
    # end, after the hook has set GTK_PATH (see packaging/apprun-bloom.sh).
    hook=squashfs-root/apprun-hooks/linuxdeploy-plugin-gtk.sh
    if ! grep -q "^export GDK_BACKEND=x11" "$hook" || ! grep -q "Adwaita:\$GTK_THEME_VARIANT" "$hook"; then
      echo "the GTK hook no longer forces X11 and Adwaita the way apprun-bloom.sh expects: check $hook" >&2
      exit 1
    fi
    sed -i "/^export GDK_BACKEND=x11/d" "$hook"
    { echo; cat /build/packaging/apprun-bloom.sh; } >> "$hook"
    # GSettings'"'"' dconf backend, so the desktop'"'"'s cursor and settings are read (see Dockerfile).
    cp /usr/lib/x86_64-linux-gnu/gio/modules/libdconfsettings.so squashfs-root/usr/lib/x86_64-linux-gnu/gio/modules/
    # Every bundled library'"'"'s copyright and licence text (see THIRD_PARTY_NOTICES.md): the
    # Ubuntu package each came from, and mpv'"'"'s own, kept by the Dockerfile.
    find squashfs-root/usr/lib -name "*.so*" -type f -printf "%f\n" | sort -u | while read -r lib; do
      path=$(find /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu /usr/lib -name "$lib" -print -quit 2>/dev/null)
      [ -n "$path" ] || continue
      dpkg -S "$path" 2>/dev/null | head -1 | cut -d: -f1
    done | sort -u > /tmp/packages
    while read -r pkg; do
      if [ -f "/usr/share/doc/$pkg/copyright" ]; then
        mkdir -p "squashfs-root/usr/share/doc/$pkg"
        cp -L "/usr/share/doc/$pkg/copyright" "squashfs-root/usr/share/doc/$pkg/copyright"
      fi
    done < /tmp/packages
    mkdir -p squashfs-root/usr/share/doc/mpv
    cp /usr/local/share/doc/mpv/* squashfs-root/usr/share/doc/mpv/
    echo "licence texts: $(find squashfs-root/usr/share/doc -name copyright | wc -l) packages, and mpv"
    head -c "$("$built" --appimage-offset)" "$built" > runtime
    rm -f /out/*.AppImage
    ARCH=x86_64 PATH="/tmp/tool/squashfs-root/appimagetool-prefix/usr/bin:$PATH" \
      appimagetool --runtime-file runtime squashfs-root "/out/$(basename "$built")" >/dev/null
    chown "$HOST_UID:$HOST_GID" /out/*.AppImage
  '

ls -lh "$out"/*.AppImage
