# Troubleshooting and FAQ

If none of this helps, please [open a bug report](../../../issues/new/choose). Include the log
(see [Getting a log](#getting-a-log)) and the line under the player for playback problems.

## Getting a log

Start Bloom from a terminal and keep its output:

```sh
./Bloom_0.1.0_amd64.AppImage 2>&1 | tee bloom.log
```

Reproduce the problem, close Bloom, and attach `bloom.log`. Look it over first for anything you'd
rather not share, such as your server's address or names.

Some lines are normal and can be ignored, such as `Failed to load module "appmenu-gtk-module"`
or `GStreamer element appsink not found`.

## Bloom doesn't start

- **"cannot execute" or nothing happens:** make sure the file is executable
  (`chmod +x Bloom_*.AppImage`).
- **"AppImages require FUSE to run":** install FUSE 2 (`fuse2` on Arch, `libfuse2t64` on Ubuntu,
  `fuse-libs` on Fedora), or run it with `--appimage-extract-and-run`.
- **"version `GLIBC_2.39' not found":** your distribution is older than Bloom supports. Bloom needs
  glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40 or later).

## Playback says "decoded in software"

That means your CPU is decoding the video instead of your graphics card.

1. **Check the decoder setting.** Settings → Playback → Hardware decoder should be **Auto**.
2. **Some files can't be decoded in hardware** on older graphics: 10-bit HEVC needs roughly an Intel
   7th-gen, AMD Ryzen 2000 or NVIDIA GTX 10-series or newer, and AV1 an Intel 11th-gen, AMD Ryzen
   6000 or NVIDIA RTX 30-series or newer. Software decoding is Bloom's fallback, so the file still
   plays.
3. **Intel and AMD: check VA-API.** Install `libva-utils` and run `vainfo`. It should name your
   driver and list profiles such as `VAProfileHEVCMain10`. If it fails, install your distribution's
   VA-API driver:
   - Intel: `intel-media-driver` (Arch, Fedora) or `intel-media-va-driver` (Debian, Ubuntu).
   - AMD: Mesa's VA-API driver (`libva-mesa-driver` on Arch; `mesa-va-drivers` on Debian and Ubuntu;
     on Fedora, `mesa-va-drivers-freeworld` from RPM Fusion for H.264 and HEVC).
4. **NVIDIA:** hardware decoding needs NVIDIA's own driver, not nouveau.

## Hybrid laptops (Intel or AMD plus NVIDIA)

Bloom runs on the integrated graphics by default, which decodes with VA-API and works well. To run
it on the NVIDIA card instead on a Wayland desktop, `prime-run` alone isn't enough; set the EGL
vendor as well:

```sh
__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/10_nvidia.json prime-run ./Bloom_0.1.0_amd64.AppImage
```

Settings → Hardware decoder then names the NVIDIA card, and the player says NVDEC.

## The page stutters when I scroll during playback

Scrolling while a video plays can drop a few frames, most noticeably on integrated graphics. The
video itself stays smooth. It's a known issue.

## My server isn't listed "On your network"

Discovery finds servers that answer Jellyfin's discovery request (UDP port 7359) on your local
network. It won't find a server when:

- the server is on another network, or reached over the internet;
- Jellyfin runs in a container without port 7359/udp published;
- a firewall on the server drops that port.

Type the address instead; everything else works the same.

## Discord doesn't show what I'm watching

- Turn on Settings → Discord → **Show what you're watching**.
- The Discord **desktop app** (or its Flatpak or Snap, or Vesktop) has to be running on the same
  computer, signed in. Discord in a web browser can't be reached.
- In Discord's settings, under **Activity Privacy**, sharing your activity with others must be on.
- If Bloom and Discord are both Flatpaks or sandboxed differently, they may not see each other.

## Notifications don't appear

Turn on Settings → Notifications → **New episodes and films**. Notifications appear only while
Bloom is open, for titles your account can watch, and need your desktop's notification service
(KDE Plasma, GNOME and most desktops have one).

## Downloads wait and don't start

- **Only download on the local network** is on, and your server is reached over the internet right
  now. Turn it off in Settings → Downloads, or connect to your home network.
- Another account's downloads wait until you switch to that account.
- A download that failed five times stops with **Try again**.

## Starting fresh

To sign out of everything and reset Bloom, close it and delete:

- `~/.local/share/dev.bloom.app` (sign-ins, settings, remembered tracks, the downloads list)
- `~/.cache/dev.bloom.app` (artwork cache)

Downloaded videos in your download folder stay where they are. Bloom won't list them again after a
reset, so delete them by hand if you don't want them.

## FAQ

**Does Bloom collect any data?**
No. Bloom talks only to your Jellyfin server, and to the Discord app on your computer if you turn
presence on. There's no telemetry, analytics or account of its own.

**Does it work without a server?**
Yes, for what you've downloaded: see [Downloads and offline](downloads.md#watching-offline).

**Does it play music?**
Not yet. Music libraries show, but there's no music player.

**Is there a Flatpak, Windows or macOS version?**
Not yet. Bloom is Linux-only for now, as an AppImage.

**Is Bloom made by the Jellyfin team?**
No. Bloom is an independent client and isn't affiliated with or endorsed by Jellyfin.
