<p align="center">
  <img src="brand/bloom-mark.svg" alt="Bloom" width="96">
</p>

<h1 align="center">Bloom</h1>

<p align="center">A fast desktop client for <a href="https://jellyfin.org">Jellyfin</a>, for Linux.</p>

<p align="center">
  <img src="docs/screenshots/home.png" alt="Bloom's Home screen, with a show in the spotlight and your libraries below" width="100%">
</p>
<p align="center">
  <img src="docs/screenshots/title.png" alt="A film's page in Bloom: its details beside the artwork, then the cast" width="100%">
</p>
<p align="center">
  <img src="docs/screenshots/player.png" alt="Bloom's player with an episode playing, the show's episodes beside it" width="100%">
</p>

Bloom plays your films and shows from a Jellyfin server in a native window, decoding video on your GPU,
with the polish of a streaming app. It's built in Rust with [Slint](https://slint.dev) and mpv, and follows Material 3.

> **Pre-release.** Bloom is a personal project. Version 0.2.0 is a new interface, built from the
> ground up, so it hasn't had long in the field: expect rough edges. Bug reports are very welcome;
> replies may take a while.

Bloom is not affiliated with or endorsed by the Jellyfin project.

## Features

- **Playback through mpv**, decoded on the GPU: NVDEC on NVIDIA, VA-API on Intel and AMD, with
  software as the fallback. Direct play whenever the file allows, the server's transcoding
  otherwise, with a quality menu.
- **A full player:** chapters, skip intro and credits, audio and subtitle tracks remembered per
  show, subtitle size and style, playback speed, up next, theater mode, full screen and a playback
  readout (<kbd>I</kbd>). Your screen won't dim, lock or sleep while something is playing.
- **Browsing:** Home with a spotlight, Continue Watching, Next Up and "Because you watched" rows;
  libraries, series, films, collections (which you can edit) and trailers; search with suggestions.
- **Music:** albums, artists and songs, with a player that follows you around the app.
- **Downloads** of films, episodes, whole seasons and collections, playable with no server.
- **Stays in sync** with the server as things change: watched state, favourites, new titles.
- **Several servers and accounts**, with servers on your network found automatically.
- **Extras:** a command palette (<kbd>Ctrl</kbd>+<kbd>K</kbd>), custom themes with a colour picker and
  [30 ready-made ones](docs/themes.md#ready-made-themes) (Catppuccin, Gruvbox, Nord, Dracula, Rosé
  Pine and more), Discord presence and desktop notifications for new episodes (both off until you
  turn them on). No frame-rate cap on the interface.

**Documentation:** [getting started](docs/getting-started.md) · [watching](docs/watching.md) ·
[downloads](docs/downloads.md) · [settings](docs/settings.md) · [custom themes](docs/themes.md) ·
[troubleshooting and FAQ](docs/troubleshooting.md)

## Install

Download the `.AppImage` from the latest [release](../../releases), then:

```sh
chmod +x Bloom-*-x86_64.AppImage
./Bloom-*-x86_64.AppImage
```

The release has a `SHA256SUMS` file to check the download against. If you used Bloom 0.1.x, your
sign-ins, settings and downloads carry over.

**Requirements**

- 64-bit x86 Linux with glibc 2.39 or newer: Ubuntu 24.04, Debian 13, Fedora 40 or later,
  current Arch, or anything as recent.
- FUSE 2 to run AppImages (`fuse2` on Arch, `libfuse2t64` on Ubuntu, `fuse-libs` on Fedora).
  Without it, run it with `--appimage-extract-and-run`.
- A Wayland or X11 desktop with OpenGL (Mesa, or NVIDIA's driver).
- A Jellyfin server (tested with 12.1 and 12.4).
- For GPU decoding on Intel or AMD, your distribution's VA-API driver (`vainfo` should list your
  GPU). NVIDIA needs only its driver.

**Tested on**

| GPU | Decoder | Setup | Bloom |
|---|---|---|---|
| NVIDIA RTX 3060 | NVDEC | Arch, KDE Plasma on Wayland | 0.2.0 |
| Intel Iris Xe (i5-1334U) | VA-API | Arch, KDE Plasma on Wayland | 0.1.x only |
| AMD Radeon Vega 10 (Ryzen 7 3750H) | VA-API | Arch, KDE Plasma on Wayland | 0.1.x only |
| NVIDIA GTX 1660 Ti, hybrid laptop | NVDEC | Arch, KDE Plasma on Wayland | 0.1.x only |

0.2.0's AppImage also starts on clean Ubuntu 24.04 and Fedora 42 containers (no GPU, so that checks
the package, not playback). The line under the player says which decoder is in use.

## Keyboard shortcuts

Press <kbd>?</kbd> in Bloom for the full list. In the player:

| Key | Action |
|---|---|
| <kbd>Space</kbd> / <kbd>K</kbd> | Play or pause |
| <kbd>←</kbd> <kbd>→</kbd> / <kbd>J</kbd> <kbd>L</kbd> | Back or forward 10 seconds |
| <kbd>↑</kbd> <kbd>↓</kbd> | Volume |
| <kbd>M</kbd> | Mute |
| <kbd>C</kbd> | Subtitles on or off |
| <kbd>I</kbd> | Playback readout |
| <kbd>[</kbd> <kbd>]</kbd> | Previous or next chapter |
| <kbd>S</kbd> | Skip intro or credits |
| <kbd>Shift</kbd>+<kbd>N</kbd> | Next episode |
| <kbd>&lt;</kbd> <kbd>&gt;</kbd> | Slower or faster |
| <kbd>0</kbd>–<kbd>9</kbd> | Jump to 0–90% |
| <kbd>T</kbd> | Theater mode |
| <kbd>F</kbd> | Full screen |
| <kbd>Esc</kbd> | Leave theater mode or full screen |

Elsewhere, <kbd>Ctrl</kbd> with <kbd>+</kbd>, <kbd>-</kbd> or the mouse wheel zooms, <kbd>/</kbd> searches,
and the mouse's back button (or <kbd>Alt</kbd>+<kbd>←</kbd>) goes back.

## Known issues

- **Hybrid laptops on Wayland:** `prime-run` alone doesn't move Bloom to the NVIDIA GPU. Add
  `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/10_nvidia.json` as well.
  Without either, Bloom runs on the integrated GPU, which works fine.
- **Seek-bar thumbnails** appear only when the server has generated trickplay images.
- **Music** has no album downloads, playlists or queue editing yet.
- **No system tray icon** yet, and the AppImage doesn't add itself to your app menu (see
  [getting started](docs/getting-started.md)).
- **Server discovery** finds servers that answer Jellyfin's UDP broadcast on your local network;
  others need their address typed in.
- Linux and x86_64 only for now.

## Privacy

- Your password is never stored. Bloom keeps the server's sign-in token in a file only your user
  can read.
- No telemetry, analytics or accounts of its own. Bloom talks only to your Jellyfin server, and
  to Discord's app on your computer if you turn presence on.
- Discord presence, when on, shows covers only from public metadata sites (TMDB, AniDB, TheTVDB,
  fanart.tv), never from your server, and can hide the title entirely.

## Building from source

You need Rust, libmpv 0.41 or newer with its headers, and fontconfig. On Arch:
`sudo pacman -S --needed base-devel rust mpv fontconfig`.

```sh
cd app
cargo run --release
```

The AppImage is built in an Ubuntu 24.04 container so it runs on older distributions too:
`packaging/build-appimage.sh` (needs Docker), then `packaging/test-appimage.sh` to try it on clean
Ubuntu and Fedora. Tests: `cargo test` in `core` and in `app`.

[DESIGN.md](DESIGN.md) describes the visual system: colour, type, motion and components.

## Contributing and reporting bugs

See [CONTRIBUTING.md](CONTRIBUTING.md) to help build Bloom. For problems, check
[troubleshooting](docs/troubleshooting.md) first, then use the [bug report form](../../issues/new/choose). It asks for your distribution, GPU,
the decoder line under the player and a log, which make most problems quick to find.

## License

Bloom is free software under the [GNU General Public License v3.0 or later](LICENSE). The AppImage
includes other open-source software under its own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
