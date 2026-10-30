# Bloom (the program)

Bloom's interface in [Slint](https://slint.dev), on `bloom-core` (`../core`). It isn't held to 60 fps:
the window is drawn when something changes, at the display's own rate. Slint draws the interface
and mpv draws the video into the same window, so nothing sits between them.

## Layout

- `ui/`: the screens and the Material 3 kit (`.slint`). `m3.slint` is the kit; `theme.slint` holds
  the colour roles; one file per screen.
- `src/`: the code behind them, a module a screen or a job: `home.rs`, `title.rs`, `watch.rs`,
  `music.rs`, `downloads.rs`, `search.rs`, `settings.rs`, `themes.rs`, `nav.rs`, `flow.rs` (sign-in)
  and so on. `mpv/` is the libmpv player drawn into the window's GL context, `awake/` keeps the
  screen awake (session bus, Wayland idle inhibitor, X server), `theme.rs` makes the colours
  (HCT) and cross-fades them, `pointer.rs` reads the mouse's back button and Ctrl+wheel from the
  window system.
- `assets/`: icons (Material Symbols as SVG), the mark and the fonts.

The interface runs on one thread. Work goes to the core's runtime and comes back with
`upgrade_in_event_loop`, which is why most modules have a plain-data half and a half that touches
the window.

## Run

```sh
cargo run --release
```

It keeps its data in `~/.local/share/dev.bloom.app` and its cache under `~/.cache/dev.bloom.app`.

## Development knobs

All optional; the list is at the top of `src/main.rs`. `BLOOM_SERVER`, `BLOOM_USER` and
`BLOOM_PASS` sign in without typing (use a test account, never a real one); `BLOOM_DATA_DIR` and
`BLOOM_CACHE_DIR` move the folders; `BLOOM_AUTOPLAY=1` plays the first film or episode on Home;
`BLOOM_SILENT=1` plays to no audio device; `BLOOM_STATS=1` prints frame rates and mpv's counters;
`BLOOM_LOG_EVENTS=1` prints the player's events; `BLOOM_GO=title|library|libraries|downloads|servers|settings|search:text`
opens a page once Home loads. `BLOOM_SCRIPT="8:click:41,150;12:type:odyssey;20:key:Escape"` sends
clicks, typing and keys through Slint's own input path (the first number is the second to send
it at; a window's content starts below any title bar, so a click's y is the screenshot's minus
the bar). That is how most of the interface was tested, headless, in a private compositor.
