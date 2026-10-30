# Third-party notices

Bloom's own code is under the [GNU General Public License v3.0 or later](LICENSE).

The AppImage also bundles open-source libraries so it runs on many distributions. Each keeps its
own licence. All of them allow redistribution in a GPL-3.0-or-later program, and the combined
AppImage is distributed under GPL-3.0-or-later.

## Main components

| Component | Version | Licence | Source |
|---|---|---|---|
| [mpv](https://mpv.io) (libmpv) | 0.41.0, built from source | GPL-2.0-or-later (mpv's default GPL build) | [v0.41.0 tag](https://github.com/mpv-player/mpv/releases/tag/v0.41.0) |
| [FFmpeg](https://ffmpeg.org) (libavcodec, libavformat, libavfilter, libswscale, …) | 6.1, Ubuntu 24.04 packages | GPL-2.0-or-later (built with `--enable-gpl`) | Ubuntu 24.04 `ffmpeg` source package |
| [Slint](https://slint.dev) | see `app/Cargo.lock` | GPL-3.0-only (used under its GPLv3 option) | [slint-ui/slint](https://github.com/slint-ui/slint) |
| [libplacebo](https://code.videolan.org/videolan/libplacebo) | Ubuntu 24.04 package | LGPL-2.1-or-later | Ubuntu 24.04 `libplacebo` source package |
| [libass](https://github.com/libass/libass) | Ubuntu 24.04 package | ISC | Ubuntu 24.04 `libass` source package |
| x264, x265, Xvid (through FFmpeg) | Ubuntu 24.04 packages | GPL-2.0-or-later | Ubuntu 24.04 source packages |
| [OpenSSL](https://openssl.org) | 3.0, Ubuntu 24.04 package | Apache-2.0 | Ubuntu 24.04 `openssl` source package |
| [libva](https://github.com/intel/libva) (used only if the system has none) | 2.20, Ubuntu 24.04 package | MIT | Ubuntu 24.04 `libva` source package |
| The Rust crates Bloom depends on | see `app/Cargo.lock` and `core/Cargo.lock` | MIT and/or Apache-2.0, among other permissive licences | [crates.io](https://crates.io) |
| [Roboto](https://github.com/googlefonts/roboto-3-classic) (Bloom's interface font), carried inside the program | 3.016 | SIL Open Font License 1.1 (`app/assets/fonts/OFL-Roboto.txt`) | [googlefonts/roboto-3-classic](https://github.com/googlefonts/roboto-3-classic) |
| [Material Symbols](https://fonts.google.com/icons) icons, as SVG | `app/assets/icons` | Apache-2.0 | [google/material-design-icons](https://github.com/google/material-design-icons) |

The AppImage carries a few dozen shared libraries in all, mostly the dependencies of the
components above, taken from Ubuntu 24.04. Their copyright and licence texts are included inside
the AppImage under `usr/share/doc/<package>/copyright`; to see them, run
`./Bloom-*-x86_64.AppImage --appimage-extract` and look in `squashfs-root/usr/share/doc/`.

Your system's graphics drivers (Mesa, NVIDIA), Wayland libraries and libva, when present, are
used from your system and aren't part of the AppImage.

## Corresponding source

- **Bloom:** this repository, at the release's tag.
- **How the AppImage is built:** [`packaging/Dockerfile`](packaging/Dockerfile) and
  [`packaging/build-appimage.sh`](packaging/build-appimage.sh), which name the exact base image,
  Ubuntu packages and mpv version used.
- **Ubuntu's libraries:** the Ubuntu 24.04 (noble) source packages, available with
  `apt source <package>` or from [launchpad.net](https://launchpad.net/ubuntu/noble).
- **mpv:** the tagged release above.

If you'd like a copy of any of the corresponding source and can't get it from those places, open
an issue and it will be provided.
