# Bloom's AppImage builder: Ubuntu 24.04, so the AppImage runs on anything with glibc 2.39 or
# newer (Ubuntu 24.04, Debian 13, Fedora 40 and later, current Arch). An AppImage built on a
# rolling distro needs that distro's newer glibc and won't start on the others.
#
# libmpv is built from source at the version Bloom is developed and tested against: Ubuntu's own
# 0.37 lacks options Bloom sets (sub-border-style for the subtitle box, the load-* switches that
# keep mpv's built-in scripts off). Ubuntu 24.04's FFmpeg 6.1 and libplacebo 6.338 meet 0.41's
# minimums.
FROM ubuntu:24.04

ARG MPV_VERSION=0.41.0
ARG NODE_VERSION=26.8.2
ARG RUST_VERSION=1.98.1

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl file git pkg-config xz-utils \
      meson ninja-build python3 \
      libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libdbus-1-dev \
      libavcodec-dev libavformat-dev libavfilter-dev libavdevice-dev libswscale-dev libswresample-dev libavutil-dev \
      libplacebo-dev libass-dev libfreetype-dev libfribidi-dev libharfbuzz-dev libuchardet-dev zlib1g-dev libjpeg-dev \
      libva-dev libvdpau-dev libdrm-dev libffmpeg-nvenc-dev libvulkan-dev libshaderc-dev \
      libegl-dev libgl-dev libgbm-dev libx11-dev libxext-dev libxinerama-dev libxrandr-dev libxss-dev libxpresent-dev \
      libwayland-dev wayland-protocols libxkbcommon-dev \
      libpulse-dev libpipewire-0.3-dev libasound2-dev \
    && rm -rf /var/lib/apt/lists/*

# libmpv only (no mpv player, no Lua: Bloom loads no scripts). No libavdevice either: it's for
# capture devices, which Bloom never opens, and it pulls in JACK and libusb, which AppImages leave
# to the system and many desktops don't have, so Bloom wouldn't start there.
RUN curl -fsSL "https://github.com/mpv-player/mpv/archive/refs/tags/v${MPV_VERSION}.tar.gz" | tar -xz -C /tmp \
    && cd "/tmp/mpv-${MPV_VERSION}" \
    && meson setup build --prefix=/usr/local --libdir=lib --buildtype=release \
         -Dlibmpv=true -Dcplayer=false -Dlua=disabled -Djavascript=disabled -Dmanpage-build=disabled \
         -Dlibavdevice=disabled \
    && meson compile -C build && meson install -C build && ldconfig \
    && mkdir -p /usr/local/share/doc/mpv \
    && cp Copyright LICENSE.GPL LICENSE.LGPL /usr/local/share/doc/mpv/ \
    && rm -rf "/tmp/mpv-${MPV_VERSION}"

RUN curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
      | tar -xJ -C /usr/local --strip-components=1 --exclude='*.md' --exclude=LICENSE

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}"

# linuxdeploy is itself an AppImage, and containers have no FUSE.
ENV APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true
# libmpv is in /usr/local/lib, which Rust's linker doesn't search on its own (ldconfig covers
# running it, and so linuxdeploy's copying of it).
ENV LIBRARY_PATH=/usr/local/lib PKG_CONFIG_PATH=/usr/local/lib/pkgconfig

# GSettings' dconf backend, which linuxdeploy doesn't bundle (it's loaded, not linked). Without it
# GSettings can't read the desktop's settings, so the cursor theme and size fall back to Adwaita's.
# build-appimage.sh copies its module in; Ubuntu's matches the bundled GLib.
RUN apt-get update && apt-get install -y --no-install-recommends dconf-gsettings-backend \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
