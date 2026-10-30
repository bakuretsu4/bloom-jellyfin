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

- **`core/`:** Bloom without a window (`bloom-core`). Talks to Jellyfin, keeps the playback
  session, handles downloads, live sync, discovery, Discord, notifications and settings. It knows
  nothing about the interface: what it needs from one (the player, the desktop, a way to tell the
  screen something changed) is the `Shell` trait in `core/src/shell.rs`.
- **`app/`:** the program (`bloom`), in Rust and [Slint](https://slint.dev). `app/ui/` has the
  screens and the Material 3 kit (`.slint`), `app/src/` the code behind them: one module a screen
  or a job (`home.rs`, `title.rs`, `watch.rs`, `music.rs`, `downloads.rs`, ...), `mpv/` the libmpv
  player drawn into the window's GL context, `awake/` keeping the screen awake, `theme.rs` the
  colours. The interface runs on one thread; work goes to the core's runtime and comes back with
  `upgrade_in_event_loop`.
- **`packaging/`:** the AppImage build (an Ubuntu 24.04 Docker image) and its clean-distro test, and the
  Flatpak (`flatpak/dev.bloom.app.yml`, built by `build-flatpak.sh`; `cargo-sources.json` is made from
  `app/Cargo.lock`, so it changes with it).
- **[DESIGN.md](DESIGN.md):** the visual system (colour, type, shape, motion and components). New UI
  should follow it.

## Setting up

You need Rust, libmpv 0.41 or newer with headers, and fontconfig. On Arch:

```sh
sudo pacman -S --needed base-devel rust mpv fontconfig
```

Then:

```sh
cd app
cargo run --release
```

It keeps its data in `~/.local/share/dev.bloom.app`. `BLOOM_DATA_DIR` and `BLOOM_CACHE_DIR` point
it elsewhere, and the list at the top of `app/src/main.rs` has the other development knobs
(including a scripting hook that drives the interface through Slint's own input path, which is how
most of it was tested).

## Checks

Please run these before opening a pull request:

```sh
cd app
cargo clippy --all-targets             # no warnings
cargo test                             # the interface's logic
cd ../core
cargo clippy --all-targets             # no warnings
cargo test                             # Jellyfin, playback, downloads, settings
```

**Live tests** run against a real Jellyfin server. Use a test account, never your real one, and put
its details in `.env.local` at the repository's root, which git ignores:

```sh
BLOOM_SERVER=http://192.168.1.20:8096
BLOOM_USER=test-account
BLOOM_PASS=...
```

```sh
cd core
set -a; . ../.env.local; set +a
cargo test live -- --ignored --nocapture
```

**Tests that need a desktop** ask the session they run in and check what it answers, so each is
ignored by default (run these in `app/`). Run the one that matches your session:

```sh
cargo test --bin bloom -- --ignored --exact \
  awake::tests::the_desktop_holds_off_while_a_video_plays          # a screensaver or power service
cargo test --bin bloom -- --ignored --exact \
  awake::wayland::tests::the_compositor_holds_off_while_a_video_plays   # a Wayland compositor
BLOOM_AWAKE_RESET_SECS=1 cargo test --bin bloom -- --ignored --exact \
  awake::x11::tests::the_x_server_stops_counting_idle_while_a_video_plays   # an X server
```

The Wayland one goes further where `BLOOM_IDLE_MARKER` names a file an idle watcher touches (for
example `swayidle -w timeout 2 'touch /tmp/idled'`): it then checks that the watcher stays quiet
while a video plays and fires once playback stops.

**The AppImage:** `packaging/build-appimage.sh` builds it from the last commit (needs Docker), and
`packaging/test-appimage.sh` checks it on clean Ubuntu 24.04 and Fedora 42 containers.
`packaging/build-flatpak.sh` builds the Flatpak (needs `flatpak install --user flathub org.flatpak.Builder
org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08`).

## Style

- Match the code around you: its naming, comment density and idioms.
- Comments explain *why*, especially anything learned the hard way (a driver quirk, a server's odd
  answer). Plain, complete sentences.
- UI text is plain and specific: say what happened and what to do, no exclamation marks.
- Keep pull requests focused on one change, with a message saying what changed and why.

## Licence

Bloom is GPL-3.0-or-later. By contributing, you agree your contribution is licensed the same way.
