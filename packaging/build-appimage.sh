#!/usr/bin/env bash
# Builds Bloom's AppImage in the Ubuntu 24.04 builder (see Dockerfile) from the last commit, and
# puts it in packaging/out/. Uncommitted changes aren't included, so what's built is what's in git.
# Needs Docker. The cargo registry and build target live in Docker volumes, so later builds are
# much faster.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/packaging/out"
mkdir -p "$out"

docker build -t bloom-appimage-builder "$root/packaging"

git -C "$root" archive --format=tar HEAD | docker run --rm -i \
  -v bloom-cargo-registry:/opt/cargo/registry \
  -v bloom-target:/build/app/target \
  -v "$out":/out \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  bloom-appimage-builder bash -c '
    set -euo pipefail
    tar -x -C /build
    cd /build/app
    cargo build --release
    version=$(sed -n "s/^version = \"\(.*\)\"/\1/p" Cargo.toml | head -1)

    cd /build
    rm -rf AppDir && mkdir AppDir
    icons=()
    for size in 16 24 32 48 64 128 256 512; do icons+=(--icon-file "brand/png/bloom-icon-$size.png"); done
    linuxdeploy --appdir AppDir --executable app/target/release/bloom \
      --desktop-file packaging/dev.bloom.app.desktop "${icons[@]}" --icon-filename dev.bloom.app
    install -Dm644 brand/bloom-icon.svg AppDir/usr/share/icons/hicolor/scalable/apps/dev.bloom.app.svg
    install -Dm644 packaging/dev.bloom.app.metainfo.xml AppDir/usr/share/metainfo/dev.bloom.app.metainfo.xml
    cd AppDir

    # Take Wayland'"'"'s libraries back out. linuxdeploy bundles Ubuntu 24.04'"'"'s (1.22), and the
    # system'"'"'s graphics drivers need the system'"'"'s: Mesa'"'"'s EGL fails to load against 1.22
    # ("undefined symbol: wl_fixes_interface"). The window system'"'"'s own libraries are loaded
    # from the system when Bloom starts, as every desktop has them.
    find . \( -type f -o -type l \) -name "libwayland-*.so*" -printf "removed %f\n" -delete
    # libva the same way, kept as a fallback. Ubuntu'"'"'s looks for drivers in its own folder
    # (/usr/lib/x86_64-linux-gnu/dri, not Arch'"'"'s /usr/lib/dri or Fedora'"'"'s /usr/lib64/dri) and
    # can'"'"'t load a newer system driver anyway: on an Arch laptop with Intel graphics vaInitialize
    # failed and every file decoded in software. AppRun uses the system'"'"'s libva when it has one,
    # and this folder only when it doesn'"'"'t.
    mkdir -p usr/lib/libva-fallback
    find usr/lib -maxdepth 1 -name "libva*.so*" -printf "moved %f to libva-fallback\n" \
      -exec mv {} usr/lib/libva-fallback/ \;
    # Our own start-up script in place of linuxdeploy'"'"'s link to the program.
    rm -f AppRun && cp /build/packaging/AppRun AppRun && chmod +x AppRun

    # Every bundled library'"'"'s copyright and licence text (see THIRD_PARTY_NOTICES.md): the
    # Ubuntu package each came from, and mpv'"'"'s own, kept by the Dockerfile.
    find usr/lib -name "*.so*" -type f -printf "%f\n" | sort -u | while read -r lib; do
      path=$(find /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu /usr/lib -name "$lib" -print -quit 2>/dev/null)
      [ -n "$path" ] || continue
      dpkg -S "$path" 2>/dev/null | head -1 | cut -d: -f1
    done | sort -u > /tmp/packages
    while read -r pkg; do
      if [ -f "/usr/share/doc/$pkg/copyright" ]; then
        mkdir -p "usr/share/doc/$pkg"
        cp -L "/usr/share/doc/$pkg/copyright" "usr/share/doc/$pkg/copyright"
      fi
    done < /tmp/packages
    mkdir -p usr/share/doc/mpv usr/share/doc/bloom
    cp /usr/local/share/doc/mpv/* usr/share/doc/mpv/
    cp /build/app/assets/fonts/OFL-Roboto.txt usr/share/doc/bloom/OFL-Roboto.txt
    echo "licence texts: $(find usr/share/doc -name copyright | wc -l) packages, and mpv"

    cd /build
    rm -f /out/*.AppImage
    ARCH=x86_64 appimagetool AppDir "/out/Bloom-$version-x86_64.AppImage" >/dev/null
    chown "$HOST_UID:$HOST_GID" /out/*.AppImage
  '

ls -lh "$out"/*.AppImage
