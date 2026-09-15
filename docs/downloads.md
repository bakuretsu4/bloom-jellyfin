# Downloads and offline

Download films and episodes to watch with no server: on a plane, on a train, or when the server's
off.

## Downloading

- **A film or an episode:** the download button on its page or under the player, or **Download
  episode** beside an episode in a season's list.
- **A whole season:** **Download season** on the show's page, above the episode list.

Downloads queue up and run in order. Follow them on the **Downloads** screen (the rail on the left,
or <kbd>Ctrl</kbd>+<kbd>K</kbd> → Downloads), which shows what's in progress, what's ready to watch,
and how much disk space Bloom's downloads, your other files and free space take up.

On that screen you can **pause**, **resume**, **cancel** or **delete** a download, change its
quality, and play anything that's ready. A download that keeps failing stops after five tries with
**Try again**.

## Quality

Set the quality for new downloads in [Settings → Downloads](settings.md#downloads), or change one
download's quality on the Downloads screen.

- **Original** is the file itself. Your server does no work, and a paused or interrupted download
  carries on from where it stopped.
- **1080p** and **720p** have your server convert the video as it downloads, at about 3.7 GB and
  1.9 GB per hour. A converted download starts again if it's paused, and its size is an estimate
  until it finishes. A file that's already lighter than that downloads as it is, since converting
  it would only make it bigger.

## Where downloads go

By default, into `~/Videos/Bloom`. Change it in [Settings → Downloads](settings.md#downloads); new
downloads go to the new place, and existing ones stay where they are.

Files are laid out the way media servers expect, so the folder reads well in a file manager and
works in other players:

```
Bloom/
├── Dune Part Two (2024)/
│   └── Dune Part Two (2024).mkv
└── Frieren/
    └── Season 1/
        ├── Frieren - S01E01 - The Journey's End.mkv
        └── Frieren - S01E01 - The Journey's End.eng.3.srt
```

Text subtitles are saved beside the video. Bloom keeps each title's page, artwork and skip markers
separately in its own data folder, so they show offline too.

## Downloading at home only

With **Only download on the local network** on, downloads wait while your server is reached over
the internet, and carry on once you're back on its network. Useful on a metered connection.

**Downloads at once** (1 to 3) runs several transfers side by side. More finishes a queue sooner,
but they share your connection, and each 1080p or 720p download is a conversion running on your
server.

## Watching offline

When your server can't be reached, Bloom still opens with what you've downloaded: find it on Home,
in search, or on the Downloads screen. Titles open with their pages, artwork, chapters and skip
buttons as they were when downloaded.

Your progress is kept while offline and sent to your server once it's reachable again, unless you've
watched further on another device in the meantime.

Downloads belong to the account that made them. Other accounts on the same computer don't see them.

## Removing downloads

- One at a time: **Delete** on the Downloads screen, or the download button on the title's page.
- All downloads from a server: remove the server on the Servers screen and choose **Remove and
  delete them**. **Remove and keep them** leaves the video files on disk.
