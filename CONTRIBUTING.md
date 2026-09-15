# Contributing to Bloom

Thanks for your interest. Bloom is a small personal project, so a few notes to make contributing
smooth for everyone.

## Before you start

- **Bugs:** use the [bug report form](../../issues/new/choose). Distribution, GPU, the decoder line
  under the player and a log make most problems quick to find.
- **Ideas and larger changes:** open an issue first to talk it through, before writing much code.
  It saves work on both sides if the change doesn't fit.
- **Small fixes** (typos, docs, obvious bugs): a pull request straight away is fine.

Replies may take a while.

## How Bloom is built

- **`app/src-tauri/`:** the Rust side (Tauri 2). Talks to Jellyfin, runs the player through
  libmpv, handles downloads, live sync, discovery, Discord and notifications.
- **`app/src/`:** the page (Svelte 5, TypeScript). Every screen and control.
- **`packaging/`:** the AppImage build (an Ubuntu 24.04 Docker image) and its clean-distro test.
- **[DESIGN.md](DESIGN.md):** the visual system (colour, type, spacing, motion and components). New UI
  should follow it.

The video is drawn by mpv into a GTK GL area underneath a transparent webview, so the page's
controls float over the picture.

## Setting up

You need Rust, Node.js with npm, libmpv 0.41 or newer with headers, and WebKitGTK 4.1. On Arch:

```sh
sudo pacman -S --needed base-devel rust nodejs npm mpv webkit2gtk-4.1
```

Then:

```sh
cd app
npm install
npm run tauri dev
```

## Checks

Please run these before opening a pull request:

```sh
cd app
npx svelte-check                       # types and Svelte warnings
cd src-tauri
cargo clippy --all-targets             # no warnings
cargo test                             # unit tests
```

**Live tests** run against a real Jellyfin server. Use a test account, never your real one, and put
its details in `.env.local` at the repository's root, which git ignores:

```sh
BLOOM_SERVER=http://192.168.1.20:8096
BLOOM_USER=test-account
BLOOM_PASS=...
```

```sh
set -a; . ../../.env.local; set +a
cargo test live -- --ignored --nocapture
```

**The AppImage:** `packaging/build-appimage.sh` builds it from the last commit (needs Docker), and
`packaging/test-appimage.sh` checks it on clean Ubuntu 24.04 and Fedora 42 containers.

## Style

- Match the code around you: its naming, comment density and idioms.
- Comments explain *why*, especially anything learned the hard way (a driver quirk, a server's odd
  answer). Plain, complete sentences.
- UI text is plain and specific: say what happened and what to do, no exclamation marks.
- Keep pull requests focused on one change, with a message saying what changed and why.

## Licence

Bloom is GPL-3.0-or-later. By contributing, you agree your contribution is licensed the same way.
