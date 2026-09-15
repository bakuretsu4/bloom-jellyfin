# Watching

## The player

Press **Play** on a film, show or episode. A show plays its next unwatched episode, and anything
you've started resumes where you left off. Your progress is saved to your server as you watch, so
you can carry on from another device.

The controls fade while the video plays and come back when you move the mouse or press a key.

| Key | Action |
|---|---|
| <kbd>Space</kbd> / <kbd>K</kbd> | Play or pause |
| <kbd>←</kbd> <kbd>→</kbd> / <kbd>J</kbd> <kbd>L</kbd> | Back or forward 10 seconds |
| <kbd>↑</kbd> <kbd>↓</kbd> | Volume |
| <kbd>M</kbd> | Mute |
| <kbd>C</kbd> | Subtitles on or off |
| <kbd>[</kbd> <kbd>]</kbd> | Previous or next chapter |
| <kbd>S</kbd> | Skip intro or credits |
| <kbd>Shift</kbd>+<kbd>N</kbd> | Next episode |
| <kbd>&lt;</kbd> <kbd>&gt;</kbd> | Slower or faster |
| <kbd>1</kbd>–<kbd>9</kbd> | Jump to 10%–90% |
| <kbd>0</kbd> / <kbd>Home</kbd> | Back to the start |
| <kbd>T</kbd> | Theater mode |
| <kbd>F</kbd> | Full screen |
| <kbd>Esc</kbd> | Leave theater mode or full screen |

Keys don't act on the player while a menu or a text field has focus.

### Three sizes

- **Default:** the player sits on the page with the title's details and episode list beside or
  below it.
- **Theater mode** (<kbd>T</kbd>): the player fills the window, with the page still a scroll away.
- **Full screen** (<kbd>F</kbd>): the whole screen.

### Seeking

Drag or click the progress bar; chapter marks show on it. Hovering shows the time and the chapter,
and a thumbnail of that moment when your server has generated **trickplay** images (Jellyfin's
dashboard → Scheduled Tasks → Generate Trickplay Images).

## Quality, audio and subtitles

Open the player's menu (the gear) for:

- **Quality:** Original plays the file as it is. 1080p (12 Mb/s) and 720p (6 Mb/s) ask your server
  to convert it, offered only when that would actually be lighter than the file. Useful on a slow
  connection.
- **Speed:** from slower to faster than normal. Speed resets to normal for the next thing you watch.
- **Audio and subtitles:** every track the file has, plus subtitle files beside it on the server.

**Your picks are remembered** per show (for all its episodes) or per film, for your account. Turning
subtitles off is remembered too. Where you haven't picked, Bloom follows your
[Playback settings](settings.md#playback) and then your server's defaults.

Subtitle size and background are in [Settings](settings.md#playback). Text subtitles, styled ones
included, follow them; picture subtitles (PGS, VobSub) keep the size the file draws them.

## Skipping intros and credits

When your server knows where an episode's intro, recap or credits are, a **Skip** button appears
at the right moment, and <kbd>S</kbd> does the same. Jellyfin learns this from media segments
(Jellyfin 10.10 and later, usually from a plugin that detects intros) or from chapters named for
them.

## Up next

When an episode ends, Bloom offers the next one. With **Autoplay next episode** on (the default),
it starts after a ten-second countdown, which <kbd>Esc</kbd> cancels; off, it waits for you.
<kbd>Shift</kbd>+<kbd>N</kbd> skips ahead at any time. The episode list beside the player shows the
whole season and the start of the next.

## Hardware decoding

Bloom decodes video on your graphics card when it can, which keeps playback smooth, the fan quiet
and the battery going. **The line under the player says what's decoding:**

- "decoded locally via NVDEC": an NVIDIA card.
- "decoded locally via VA-API": Intel or AMD graphics.
- "decoded in software": the CPU. Fine for most files on a desktop, heavy for 4K or on a laptop.

Bloom plays the original file whenever your computer can decode it (**direct play**), so your
server has no work to do. Only when it can't, or when you pick a lower quality, does the server
convert the video.

If a file you expect your GPU to handle says "decoded in software", see
[Playback uses the CPU](troubleshooting.md#playback-says-decoded-in-software).

## Trailers

Films and shows with trailers on your server get a **Trailer** button. With
[yt-dlp](https://github.com/yt-dlp/yt-dlp) installed, trailers play inside Bloom's player;
without it, or if yt-dlp can't find the video within 30 seconds, the trailer opens in your
browser.

## Discord

Turn on **Settings → Discord → Show what you're watching** to show the title, episode, cover and
time left on your Discord profile while something plays. It works through the Discord desktop app
(including its Flatpak and Snap) or Vesktop, running on the same computer, and clears when
playback stops.

The cover comes from a public metadata site such as TMDB, never from your server, so friends can't
see your server's address. Turn **Show the title** off to show only that you're watching a show
or a film. Trailers are never shown.
