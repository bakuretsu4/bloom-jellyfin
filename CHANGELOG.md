# Changelog

All notable changes to Bloom. The format follows [Keep a Changelog](https://keepachangelog.com),
and versions follow [Semantic Versioning](https://semver.org).

## [0.2.0] - 2026-10-30

A new interface. Still a pre-release.

### Changed

- **A new interface.** Bloom is now built in Rust with [Slint](https://slint.dev) instead of Tauri
  and a web page, and follows Material 3. The interface has no frame-rate cap, video is drawn by mpv
  into the window's own GL context, and the AppImage is about 100 MB. The settings, sign-ins and
  downloads of 0.1.x carry over.
- **Title pages** are a split hero: the words on a panel at the left, the artwork at the right, the
  facts in a strip under it. A card's picture grows into the page it opens.
- **Home** has a spotlight with previous and next buttons and a progress bar per slide.
- **The top bar** shows your server and account as two pills, with your profile picture.
- **A new logo:** a blossom with a play button in its centre.

### Added

- **Music:** albums, artists and songs, with a player at the bottom of every page, and music in
  search.
- **"Because you watched"** rows on Home.
- **Collections:** add a title to one, take it out, or make a new one; remove titles on a collection's
  page.
- **Downloads** of a whole show or collection, and a show's downloaded episodes grouped under one
  heading.
- **A playback readout** on <kbd>I</kbd>: codec, decoder, dropped frames, bit rates, cache, A/V sync.
- **A colour picker** in the theme editor, and 30 ready-made themes built in.
- **Clicking a new-media notification** opens the title.
- **The mouse's back button** goes back, and Ctrl with the wheel zooms.
- Saved accounts show their own pictures.

### Fixed

- **The screen sleeping mid-film.** While a video plays, Bloom now asks to keep the screen on and
  the machine awake, and lets go on a pause or a stop. Before, a film watched without touching the
  keyboard could be interrupted by the screen dimming, locking or the machine suspending, full
  screen included. It asks whichever way your desktop understands: the usual screensaver and power
  services (KDE, GNOME, Xfce), the Wayland idle inhibitor (sway, Hyprland, river), or the X
  server's idle count (i3, bspwm, dwm).
- **Trailers in the AppImage.** They neither played nor opened in the browser, even with yt-dlp
  installed. Programs Bloom starts (yt-dlp, the desktop's browser opener, the font list) inherited
  the AppImage's own library and Python paths, which broke them; they now run with the system's
  environment.

### Added

- When a trailer opens in the browser (without yt-dlp), the title page says so under its buttons.
- [18 ready-made themes](docs/themes.md#ready-made-themes): Catppuccin (Mocha, Macchiato, Frappé,
  Latte), Gruvbox Dark and Light, Nord, Dracula, Tokyo Night, Rosé Pine (main, Moon, Dawn),
  Everforest Dark and Light, Kanagawa, One Dark, and Solarized Dark and Light.

### Security

- A Content Security Policy for the app's page: scripts only from Bloom itself, artwork only
  through Bloom, and no connections beyond Bloom's own bridge.

### Changed

- Documentation: troubleshooting for trailers and for a screen that still sleeps, updating through
  Gear Lever, and instructions that no longer name a specific version.

## [0.1.0] - 2026-09-15

The first public release, as a pre-release.

### Added

- Playback through mpv, decoded on the GPU (NVDEC on NVIDIA, VA-API on Intel and AMD) with software
  as the fallback; direct play whenever the file allows, the server's transcoding otherwise.
- A full player: chapters, skip intro and credits, remembered audio and subtitle tracks, subtitle
  size and background, speed, up next, theater mode and full screen, seek-bar thumbnails from
  trickplay.
- Home with a spotlight, Continue Watching and Next Up; libraries, series and film pages, hover
  cards, collections, trailers, and search with suggestions.
- Downloads of films, episodes and whole seasons, playable offline.
- Live sync with the server, several servers and accounts, and servers found on the local network.
- A command palette, custom themes, Discord presence and new-episode notifications.
- An AppImage for 64-bit Linux with glibc 2.39 or newer, tested on NVIDIA, Intel and AMD graphics.

[Unreleased]: https://github.com/bakuretsu4/bloom-jellyfin/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/bakuretsu4/bloom-jellyfin/releases/tag/v0.1.0
