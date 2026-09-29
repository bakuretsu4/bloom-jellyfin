# Flutter frame-rate spike

Answers two questions before any port of Bloom to Flutter is started:

1. Does a Flutter Linux window run at your monitor's refresh rate (240 Hz), not 60?
2. Does it still, with mpv video playing underneath Material 3 widgets that are animating?

Not Bloom code. Nothing here is used by `app/`.

## Set up (once)

Install Flutter's Linux desktop prerequisites and the Flutter SDK. On Arch:

```sh
sudo pacman -S --needed base-devel clang cmake ninja pkgconf gtk3 mpv
git clone https://github.com/flutter/flutter.git -b stable ~/flutter
export PATH="$HOME/flutter/bin:$PATH"     # add this to your shell profile too
flutter doctor                            # fix anything it flags under "Linux toolchain"
flutter config --enable-linux-desktop
```

## Run

```sh
cd spikes/flutter-fps
flutter create --platforms=linux .        # generates the linux/ runner; doesn't touch lib/
flutter pub get
flutter run -d linux --release            # release matters: debug mode is far slower
```

Optionally play a local file instead of the sample download:

```sh
VIDEO=/path/to/some/film.mkv flutter run -d linux --release
```

## What to report

The box in the top right shows frames per second and the worst frame time for the last second.
Note the numbers for each:

1. Video playing, everything animating (the default).
2. "Hide video": the same window without mpv.
3. Scroll the card row with the mouse wheel while the video plays.

Also tell me:

- Your session type (`echo $XDG_SESSION_TYPE`: x11 or wayland), your desktop, and your GPU.
- Whether it's noticeably smoother than Bloom's current UI.
- If it caps at 60, try `GDK_BACKEND=x11 flutter run -d linux --release` and
  `GDK_BACKEND=wayland ...` and report both.
- Any errors from the video (`media_kit` needs libmpv, which the `mpv` package provides).
