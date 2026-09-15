# Settings

Open Settings from the rail on the left, or jump straight to a section with
<kbd>Ctrl</kbd>+<kbd>K</kbd> and its name. Changes apply at once, and settings are kept per
computer rather than per account.

## Playback

Tracks and quality you pick in the player for one show or film always win over these.

| Setting | Options | Default | What it does |
|---|---|---|---|
| **Hardware decoder** | Auto, NVDEC, VA-API, Software | Auto | Auto picks the best decoder for your graphics (named beside it) and falls back to software for anything it can't decode. NVDEC is NVIDIA's; VA-API is Intel's and AMD's. See [hardware decoding](watching.md#hardware-decoding). |
| **Prefer direct play** | On, off | On | Plays the original file whenever this computer can decode it. Off, the server converts everything, which costs it CPU and costs you picture. |
| **Starting quality** | Original, 1080p, 720p | Original | The quality the player opens at. Below Original, your server converts files heavier than that. |
| **Audio language** | Server's choice, or a language | Server's choice | For files with several audio tracks, when you haven't picked one for that show or film. |
| **Subtitles** | Server's choice, Off, Forced only, Always | Server's choice | Forced only shows signs and lines in another language. Server's choice follows your Jellyfin account's settings. |
| **Subtitle language** | Same as the audio, or a language | Same as the audio | Shown with Forced only or Always: which language's subtitles to use when a file has several. |
| **Subtitle size** | Small, Normal, Large, Huge | Normal | For text subtitles, styled ones included. Picture subtitles (PGS, VobSub) keep their own size. |
| **Subtitle background** | Outline, Dark box | Outline | A box reads better over bright scenes. Only plain subtitles take it; styled ones keep their look. |
| **Autoplay next episode** | On, off | On | Starts the next episode ten seconds after one ends. Off, the player waits for you. |

## Notifications

| Setting | Default | What it does |
|---|---|---|
| **New episodes and films** | Off | A desktop notification when your server adds an episode or film your account can watch, while Bloom is open. A show's new episodes come as one notification, and a big batch as one in all. |

## Discord

| Setting | Default | What it does |
|---|---|---|
| **Show what you're watching** | Off | While something plays, your Discord profile shows its cover, title, episode and time left, through the Discord app on this computer. Clears when playback stops. Nothing is sent when Discord isn't running. |
| **Show the title** | On | Shown once the above is on. Off, your profile only says you're watching a show or a film, with no cover. |

Covers come from public metadata sites (such as TMDB), never your server. More in
[Watching → Discord](watching.md#discord).

## Downloads

| Setting | Options | Default | What it does |
|---|---|---|---|
| **Location** | A folder | `~/Videos/Bloom` | Where new downloads go, with their subtitles. Existing downloads stay where they are. Type a folder and choose **Use**; Bloom checks it can write there. |
| **Download quality** | Original, 1080p, 720p | Original | Original is the file itself and resumes where it stopped. 1080p and 720p are converted by your server as they download (about 3.7 GB and 1.9 GB an hour) and start again if paused. Files already lighter download as they are. |
| **Downloads at once** | 1, 2, 3 | 1 | More finishes a queue sooner, but downloads share your connection, and each 1080p or 720p one is a conversion on your server. |
| **Only download on the local network** | On, off | Off | Downloads wait while your server is reached over the internet rather than your local network. |

More in [Downloads and offline](downloads.md).

## Appearance

| Setting | Options | Default | What it does |
|---|---|---|---|
| **Theme** | Auto, Light, Dark | Auto | Auto follows your desktop's light or dark preference. Choosing one leaves a custom theme. |
| **Custom themes** | Your themes | None | Your own background, panel, text and accent colours and font. See [Custom themes](themes.md). |
| **Accent** | Tally amber, Signal green, Steel blue, Clay red | Tally amber | The highlight colour. Each has a darker shade in the light theme, so it stays readable. The player's controls stay amber, and a custom theme brings its own accent. |
| **Zoom** | 50% to 200% | 100% | The size of everything. Also <kbd>Ctrl</kbd>+<kbd>+</kbd> and <kbd>Ctrl</kbd>+<kbd>−</kbd>, <kbd>Ctrl</kbd>+<kbd>0</kbd> to reset, or <kbd>Ctrl</kbd> with the scroll wheel. |
| **Reduce motion** | On, off | Off | Stops animations and transitions, and keeps the Home spotlight on one title. Your desktop's own reduce-motion setting does the same. Buffering still shows. |

## About

Bloom's version, the server you're on and its Jellyfin version, the account you're signed in as,
the graphics Bloom is drawing with, and whether the player has started. Useful when
[reporting a bug](troubleshooting.md).

## Where settings are kept

In `~/.local/share/dev.bloom.app/settings.json`. Your volume is remembered there too.
