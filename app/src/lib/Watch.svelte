<script lang="ts" module>
  import type { Chapter } from "./api";
  import { languageName } from "./language";

  const CODECS: Record<string, string> = {
    hevc: "HEVC",
    h264: "H.264",
    av1: "AV1",
    vp9: "VP9",
    mpeg2video: "MPEG-2",
    aac: "AAC",
    ac3: "AC-3",
    eac3: "E-AC-3",
    truehd: "TrueHD",
    dts: "DTS",
    flac: "FLAC",
    opus: "Opus",
    mp3: "MP3",
    vorbis: "Vorbis",
    subrip: "SRT",
    ass: "ASS",
    ssa: "SSA",
    webvtt: "WebVTT",
    mov_text: "Timed text",
    hdmv_pgs_subtitle: "PGS",
    pgssub: "PGS",
    dvd_subtitle: "VobSub",
    dvdsub: "VobSub",
    dvb_subtitle: "DVB",
  };
  const codecLabel = (codec: string) => CODECS[codec.toLowerCase()] ?? codec.toUpperCase();
  /** Ids of a transcode's image subtitles start here. Mirrors SERVER_SUBTITLE_BASE in playback.rs. */
  const SERVER_SUBTITLE_BASE = 1_000_000;
  const channelLabel = (n: number) => ({ 1: "mono", 2: "2.0", 6: "5.1", 8: "7.1" })[n] ?? `${n} ch`;


  function clock(seconds: number | null): string {
    if (seconds == null || !Number.isFinite(seconds)) return "--:--";
    const total = Math.max(0, Math.floor(seconds));
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = String(total % 60).padStart(2, "0");
    return h ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
  }

  function runtime(seconds: number): string {
    const minutes = Math.round(seconds / 60);
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return h ? (m ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
  }

  const clamp = (n: number, lo: number, hi: number) => Math.min(Math.max(n, lo), hi);

  // Mirrors Quality::limits in src-tauri/src/playback.rs.
  const QUALITIES = [
    { id: "1080p", width: 1920, height: 1080, bitrate: 12_000_000 },
    { id: "720p", width: 1280, height: 720, bitrate: 6_000_000 },
  ] as const;

  /** The last chapter starting before `seconds` (chapters are in order). */
  function lastChapterBefore(chapters: Chapter[], seconds: number, inclusive: boolean): Chapter | null {
    let found: Chapter | null = null;
    for (const c of chapters) {
      if (inclusive ? c.startSeconds > seconds : c.startSeconds >= seconds) break;
      found = c;
    }
    return found;
  }

  const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];
  const speedLabel = (speed: number) => (Math.abs(speed - 1) < 0.01 ? "Normal" : `${Number(speed.toFixed(2))}×`);
  /** A seek-bar thumbnail's width in the tooltip. */
  const THUMB_WIDTH = 176;

  /** Below this window width the list moves from beside the player to under it. */
  const SIDE_BY_SIDE = 1180;
</script>

<script lang="ts">
  import { tick, untrack } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import * as api from "./api";
  import Art from "./Art.svelte";
  import DownloadButton from "./DownloadButton.svelte";
  import { downloads } from "./downloads.svelte";
  import { forgetLive, live } from "./live.svelte";
  import Icon from "./Icon.svelte";
  import EpisodeHover from "./EpisodeHover.svelte";
  import HoverPanel from "./HoverPanel.svelte";
  import MediaSpecs from "./MediaSpecs.svelte";
  import Select from "./Select.svelte";
  import TitleHover from "./TitleHover.svelte";
  import { HoverIntent } from "./hover.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import { prefs } from "./settings.svelte";

  let {
    itemId,
    onClose,
    onSignedOut,
    onOpenItem,
    onOpenDownloads,
    trailerIndex = null,
  }: {
    itemId: string;
    /** Given, the screen plays that trailer of the item rather than the item. */
    trailerIndex?: number | null;
    onClose: () => void;
    onSignedOut: () => void;
    onOpenItem: (itemId: string) => void;
    onOpenDownloads: () => void;
  } = $props();

  /** A trailer has none of the item's chapters, segments, thumbnails, quality or actions. */
  let isTrailer = $derived(trailerIndex != null);

  type Mode = "default" | "theater" | "fullscreen";

  let mode = $state<Mode>("default");
  let now = $state<api.NowPlaying | null>(null);
  let player = $state<api.PlayerState | null>(null);
  let tracks = $state<api.Track[]>([]);
  let error = $state("");
  let closing = $state(false);
  let awake = $state(true);
  let menuOpen = $state(false);
  /** The episode after this one, when there is one. */
  let upNext = $state<api.UpNext | null>(null);
  /** Seconds left before the next episode starts, while the up-next panel is showing. */
  let countdown = $state<number | null>(null);
  /** Where up next is showing: over the closing credits while they play, or after the file ends. */
  let upNextAt = $state<"credits" | "end">("end");
  /** Watch credits was chosen, so up next waits for the end of this file. */
  let creditsDismissed = $state(false);
  /** The server's seek-bar thumbnails for what's playing, where it has made them. */
  let trickplay = $state<api.Trickplay | null>(null);
  /** Carries over to the next episode on this screen. */
  let quality = $state<api.Quality>(prefs.current?.maxQuality ?? "original");
  /** Settings, Autoplay next episode: off, the up-next panel waits instead of counting down. */
  let autoplay = $derived(prefs.current?.autoplayNext ?? true);
  /** The playing item's page data: its actions, chapters, cast and media info. */
  let detail = $state<api.ItemDetail | null>(null);
  let queue = $state<api.Queue | null>(null);
  let favorite = $state(false);
  let played = $state(false);
  let acting = $state(false);
  let actionError = $state("");
  let innerWidth = $state(0);
  /** The intro, recap, credits and preview, where known. */
  let segments = $state<api.Segment[]>([]);
  /** For an episode: the season shown in the list, which the season picker can change. */
  let seasonList = $state<api.SeasonList | null>(null);
  let listSeriesId: string | null = null;
  let listSeasonId = $state<string | null>(null);
  /** The page scrolls as a whole; the list beside the player scrolls on its own. */
  let scroller = $state<HTMLDivElement>();
  let side = $state<HTMLElement>();
  const listHover = new HoverIntent();
  const listAnchors: Record<string, HTMLElement> = {};

  let box = $state<HTMLDivElement>();
  let trackBar = $state<HTMLDivElement>();
  let menu = $state<HTMLDivElement>();
  let menuButton = $state<HTMLButtonElement>();

  // Scrubbing previews locally and seeks once, on release.
  let dragging = $state(false);
  let dragTime = $state(0);
  let hover = $state<{ x: number; time: number } | null>(null);
  // Until mpv reports the new position, show where the seek was aimed rather than jumping back.
  let seekTarget = $state<number | null>(null);
  let seekTimer: ReturnType<typeof setTimeout> | undefined;

  let theaterBeforeFullscreen = false;
  let lastSubtitle: number | null = null;

  let paused = $derived(player?.paused ?? false);
  let duration = $derived(player?.duration ?? now?.durationSeconds ?? null);
  let position = $derived(seekTarget ?? player?.position ?? now?.startSeconds ?? 0);
  let shown = $derived(dragging ? dragTime : position);
  let playedPct = $derived(duration ? clamp((shown / duration) * 100, 0, 100) : 0);
  let bufferedPct = $derived(duration && player?.cacheEnd ? clamp((player.cacheEnd / duration) * 100, 0, 100) : 0);
  let started = $derived(!!player?.started);
  let waiting = $derived(!error && (countdown === null || upNextAt === "credits") && (!started || !!player?.buffering));
  let volume = $derived(player?.volume ?? 100);
  let muted = $derived(player?.muted ?? false);
  let speed = $derived(player?.speed ?? 1);
  let showChrome = $derived(awake || paused || waiting || !!error || menuOpen || dragging);

  let audioTracks = $derived(tracks.filter((t) => t.kind === "audio"));
  let subtitleTracks = $derived(tracks.filter((t) => t.kind === "sub"));
  let videoTrack = $derived(tracks.find((t) => t.kind === "video" && t.selected) ?? tracks.find((t) => t.kind === "video"));
  let audioTrack = $derived(audioTracks.find((t) => t.id === player?.audioTrack));

  let chapters = $derived(isTrailer ? [] : (detail?.chapters ?? []).filter((c) => duration == null || c.startSeconds < duration));
  let hasChapters = $derived(chapters.length > 1);
  let hoverChapter = $derived.by(() => {
    if (!hasChapters || !hover) return null;
    const t = dragging ? dragTime : hover.time;
    return lastChapterBefore(chapters, t, true);
  });
  let sideBySide = $derived(mode === "default" && innerWidth >= SIDE_BY_SIDE);
  /** The segment the playhead is in, until its last second. */
  let skippable = $derived(
    started && countdown === null && !error
      ? (segments.find((s) => position >= s.startSeconds && position < s.endSeconds - 1) ?? null)
      : null,
  );
  /** Where the closing credits start: the first credits segment in the last 40% of the file. An
   * opening song marked as credits isn't the end of the episode. */
  let creditsStart = $derived.by(() => {
    if (!duration) return null;
    const end = duration;
    return segments.find((s) => s.kind === "credits" && s.startSeconds >= end * 0.6)?.startSeconds ?? null;
  });
  /** Until the item is known, assume an episode: most plays are. */
  let isEpisode = $derived((now?.kind ?? "Episode") === "Episode");
  let seasonOptions = $derived(
    (seasonList?.seasons ?? []).map((s) => ({
      value: s.id,
      label: s.name,
      detail: `${s.episodeCount} ${s.episodeCount === 1 ? "episode" : "episodes"}`,
    })),
  );

  /** Qualities below the file's own: a smaller frame or a lighter bitrate. */
  let qualityOptions = $derived.by(() => {
    // A downloaded copy is one fixed file.
    if (!now || now.downloaded || now.kind === "Trailer") return [];
    const src = now.source;
    return QUALITIES.filter(
      (q) => !src || (src.width ?? 0) > q.width || (src.height ?? 0) > q.height || (src.bitrate ?? 0) > q.bitrate,
    );
  });
  let sourceLabel = $derived.by(() => {
    const src = now?.source;
    if (!src) return "";
    const parts: string[] = [];
    if (src.width && src.height) parts.push(`${src.width}×${src.height}`);
    if (src.codec) parts.push(codecLabel(src.codec));
    if (src.bitrate) parts.push(`${(src.bitrate / 1e6).toFixed(1)} Mb/s`);
    return parts.join(", ");
  });

  let meta = $derived.by(() => {
    const parts: string[] = [];
    if (now?.subtitle) parts.push(now.subtitle);
    if (duration) parts.push(runtime(duration));
    if (detail?.genres.length) parts.push(detail.genres.slice(0, 3).join(", "));
    if (detail?.communityRating) parts.push(`${detail.communityRating.toFixed(1)}/10`);
    return parts;
  });

  let readout = $derived.by(() => {
    const parts: string[] = [];
    if (videoTrack?.codec) parts.push(codecLabel(videoTrack.codec));
    if (videoTrack?.width && videoTrack.height) parts.push(`${videoTrack.width}×${videoTrack.height}`);
    if (videoTrack?.fps) parts.push(`${Number(videoTrack.fps.toFixed(3))} fps`);
    if (audioTrack?.codec) {
      parts.push(audioTrack.channels ? `${codecLabel(audioTrack.codec)} ${channelLabel(audioTrack.channels)}` : codecLabel(audioTrack.codec));
    }
    return parts;
  });

  function trackName(track: api.Track, index: number): string {
    const language = languageName(track.lang);
    if (track.title && language && !track.title.toLowerCase().includes(language.toLowerCase())) {
      return `${language}, ${track.title}`;
    }
    // A subtitle saved with a download can have an empty title.
    return track.title || language || `Track ${index + 1}`;
  }

  function trackDetail(track: api.Track): string {
    const parts: string[] = [];
    if (track.codec) parts.push(track.kind === "audio" && track.channels ? `${codecLabel(track.codec)} ${channelLabel(track.channels)}` : codecLabel(track.codec));
    if (track.forced) parts.push("forced");
    if (track.hearingImpaired) parts.push("SDH");
    if (track.external) parts.push("external");
    if (track.kind === "sub" && track.id >= SERVER_SUBTITLE_BASE) parts.push("drawn into the picture by the server");
    return parts.join(", ");
  }

  function decoderLabel(decoder: string | null | undefined): string {
    if (!decoder) return "decoded in software";
    if (decoder.startsWith("nvdec") || decoder.startsWith("cuda")) return "decoded locally via NVDEC";
    if (decoder.startsWith("vaapi")) return "decoded locally via VA-API";
    if (decoder.startsWith("vulkan")) return "decoded locally via Vulkan";
    return `decoded locally via ${decoder}`;
  }

  // --- the player's place on screen

  /** Tell mpv where the unpainted box is, so the picture fills exactly that. */
  function syncViewport() {
    if (!box) return;
    const r = box.getBoundingClientRect();
    const w = window.innerWidth;
    const h = window.innerHeight;
    if (!w || !h) return;
    // Rust crops the picture should the box ever run past the window's edge.
    api.setViewport(r.left / w, r.top / h, (w - r.right) / w, (h - r.bottom) / h, w / h).catch(() => {});
  }

  $effect(() => {
    if (!box) return;
    const observer = new ResizeObserver(syncViewport);
    observer.observe(box);
    window.addEventListener("resize", syncViewport);
    syncViewport();
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", syncViewport);
    };
  });

  // mpv only knows the picture's shape, which the crop needs, once the first frame is up.
  $effect(() => {
    if (started) untrack(syncViewport);
  });

  $effect(() => {
    document.documentElement.dataset.playerMode = mode;
    // A mode change can move the box without resizing it (the top bar appearing or going).
    tick().then(() => requestAnimationFrame(syncViewport));
    return () => {
      delete document.documentElement.dataset.playerMode;
    };
  });

  /** Set while the window goes to or from full screen, so a second press waits for the first. */
  let changingMode = false;

  // The layout changes at once and the picture follows it within a frame or two, which reads as a
  // clean switch. (A shutter used to close over the box meanwhile; watching, the blank was worse
  // than the catch-up it hid.) Chrome dims or returns alongside (App.svelte).
  async function setMode(next: Mode) {
    if (next === mode || changingMode) return;
    changingMode = true;
    const windowChanges = next === "fullscreen" || mode === "fullscreen";
    if (next === "fullscreen") theaterBeforeFullscreen = mode === "theater";
    mode = next;
    // A bigger player is for watching: bring it back into view.
    if (next !== "default") scroller?.scrollTo({ top: 0 });
    if (windowChanges) await api.setFullscreen(next === "fullscreen").catch(() => {});
    changingMode = false;
    wake();
  }

  function toggleTheater() {
    if (mode !== "fullscreen") void setMode(mode === "theater" ? "default" : "theater");
  }

  function toggleFullscreen() {
    void setMode(mode === "fullscreen" ? (theaterBeforeFullscreen ? "theater" : "default") : "fullscreen");
  }

  // --- playback

  /** Bumped by every play request, so a slow answer to an older one is dropped. */
  let generation = 0;
  /** Bumped when the item changes, for what belongs to one item (its page data, up next). */
  let itemGeneration = 0;
  let disposed = false;

  /** Play an item on this screen. Also how the next episode starts, so the mode carries over. */
  async function start(id: string) {
    const mine = ++generation;
    const item = ++itemGeneration;
    clearInterval(countdownTimer);
    countdown = null;
    now = null;
    player = null;
    tracks = [];
    error = "";
    upNext = null;
    seekTarget = null;
    menuOpen = false;
    detail = null;
    queue = null;
    segments = [];
    actionError = "";
    upNextAt = "end";
    creditsDismissed = false;
    trickplay = null;
    try {
      const playing = trailerIndex != null ? await api.playTrailer(id, trailerIndex) : await api.play(id, quality);
      if (disposed || mine !== generation) return;
      now = playing;
      loadItem(playing, item);
    } catch (e) {
      if (disposed || mine !== generation || api.isSuperseded(e)) return;
      if (api.isSignedOut(e)) onSignedOut();
      else if (trailerIndex != null) {
        // yt-dlp couldn't find it: the browser still can.
        const opened = await api
          .openTrailer(id, trailerIndex)
          .then(() => true)
          .catch(() => false);
        error = opened
          ? "Couldn't play the trailer here, so it opened in your browser."
          : `Couldn't play the trailer: ${api.message(e)}`;
      } else error = api.message(e);
    }
  }

  function loadItem(playing: api.NowPlaying, item: number) {
    const current = () => !disposed && item === itemGeneration;
    if (playing.kind === "Episode") {
      api
        .nextEpisode(playing.itemId)
        .then((next) => {
          if (current()) upNext = next;
        })
        .catch(() => {});
    }
    const episode = playing.kind === "Episode";
    if (!episode) {
      seasonList = null;
      listSeriesId = null;
      listSeasonId = null;
    }
    // A trailer's thumbnails and segments would be the item's, not the trailer's.
    if (playing.kind !== "Trailer") {
      // No thumbnails is normal: the server only makes them where a library has it turned on.
      api
        .trickplay(playing.itemId)
        .then((loaded) => {
          if (current()) trickplay = loaded;
        })
        .catch(() => {});
      api
        .skipSegments(playing.itemId)
        .then((loaded) => {
          if (current()) segments = loaded;
        })
        .catch(() => {});
    }
    api
      .itemDetail(playing.itemId)
      .then((loaded) => {
        if (!current()) return;
        detail = loaded;
        favorite = loaded.favorite;
        played = loaded.played;
        // Reloaded even when the episode was already listed, so watched marks stay current.
        // The old list stays up meanwhile, so moving to the next episode doesn't flash.
        if (episode && loaded.series && loaded.seasonId) void loadSeason(loaded.series.id, loaded.seasonId, item);
        else if (episode) seasonList = { seasons: [], seasonId: "", entries: [], next: null };
      })
      .catch(() => {
        if (current() && episode && !seasonList) seasonList = { seasons: [], seasonId: "", entries: [], next: null };
      });
    if (!episode) {
      api
        .watchQueue(playing.itemId)
        .then((loaded) => {
          if (current()) queue = loaded;
        })
        .catch(() => {
          if (current()) queue = { heading: "More like this", entries: [] };
        });
    }
  }

  async function loadSeason(seriesId: string, seasonId: string, item = itemGeneration) {
    listSeriesId = seriesId;
    listSeasonId = seasonId;
    try {
      const loaded = await api.seasonList(seriesId, seasonId);
      if (!disposed && item === itemGeneration && listSeasonId === seasonId) {
        seasonList = loaded;
        // The server's own list can name the same season by another id.
        listSeasonId = loaded.seasonId;
      }
    } catch (e) {
      if (disposed || item !== itemGeneration || listSeasonId !== seasonId) return;
      if (api.isSignedOut(e)) onSignedOut();
      else seasonList = { seasons: seasonList?.seasons ?? [], seasonId, entries: downloadedEpisodes(seriesId), next: null };
    }
  }

  /** With no server, the list beside the player is the show's downloaded episodes. */
  function downloadedEpisodes(seriesId: string): api.QueueEntry[] {
    return downloads.list
      .filter((d) => d.seriesId === seriesId && d.status === "done")
      .sort((a, b) => (a.seasonNumber ?? 0) - (b.seasonNumber ?? 0) || (a.episodeNumber ?? 0) - (b.episodeNumber ?? 0))
      .map((d) => ({
        id: d.itemId,
        title: d.subtitle ?? d.title,
        meta: null,
        image: d.image,
        runtimeMinutes: d.runtimeSeconds ? Math.round(d.runtimeSeconds / 60) : null,
        progress: d.runtimeSeconds && d.positionSeconds > 0 && !d.played ? d.positionSeconds / d.runtimeSeconds : null,
        played: d.played,
        overview: null,
        premiereDate: null,
      }));
  }

  function downloadFailed(e: unknown) {
    if (api.isSignedOut(e)) onSignedOut();
    else actionError = api.message(e);
  }

  function chooseSeason(seasonId: string) {
    if (listSeriesId) void loadSeason(listSeriesId, seasonId);
  }

  function playEntry(id: string) {
    listHover.close();
    if (id !== now?.itemId) void start(id);
  }

  async function toggleEntryPlayed(entry: api.QueueEntry) {
    forgetLive(entry.id);
    try {
      entry.played = await api.setPlayed(entry.id, !entry.played);
      if (entry.played) entry.progress = null;
    } catch (e) {
      if (api.isSignedOut(e)) onSignedOut();
    }
  }

  // Keep the playing episode in view in the side list: centred when it's in the season shown,
  // otherwise the list starts at the top.
  $effect(() => {
    const list = seasonList;
    void now?.itemId;
    if (!side || !list) return;
    tick().then(() => {
      if (!side) return;
      const current = side.querySelector<HTMLElement>(".qitem.is-current");
      side.scrollTop = current ? current.offsetTop - side.clientHeight / 2 + current.offsetHeight / 2 : 0;
    });
  });

  /** Ask for the stream again from where playback is: another quality, or audio a transcode left out. */
  async function restream(next: api.Quality) {
    if (!now) return;
    const id = now.itemId;
    const at = position;
    const mine = ++generation;
    quality = next;
    menuOpen = false;
    seekTarget = null;
    player = null;
    tracks = [];
    try {
      const playing = await api.play(id, next, at);
      if (!disposed && mine === generation) now = playing;
    } catch (e) {
      if (disposed || mine !== generation || api.isSuperseded(e)) return;
      if (api.isSignedOut(e)) onSignedOut();
      else error = api.message(e);
    }
  }

  function chooseQuality(next: api.Quality) {
    if (next === quality) menuOpen = false;
    else void restream(next);
  }

  function chooseTrack(kind: "audio" | "subtitle", id: number | null) {
    wake();
    api
      .setTrack(kind, id)
      .then((again) => {
        if (again) void restream(quality);
      })
      .catch(() => {});
  }

  async function toggle(kind: "favorite" | "played") {
    if (!detail || acting) return;
    acting = true;
    actionError = "";
    forgetLive(detail.id);
    try {
      if (kind === "favorite") favorite = await api.setFavorite(detail.id, !favorite);
      else played = await api.setPlayed(detail.id, !played);
    } catch (e) {
      if (api.isSignedOut(e)) onSignedOut();
      else actionError = api.message(e);
    } finally {
      acting = false;
    }
  }

  $effect(() => {
    const listeners: Promise<UnlistenFn>[] = [
      api.onPlayerState((state) => {
        player = state;
        if (seekTarget !== null && Math.abs(state.position - seekTarget) < 1.5) seekTarget = null;
      }),
      api.onPlayerTracks((list) => (tracks = list)),
      api.onPlayerEnded((ended) => {
        if (ended.reason === "error") {
          error = ended.message ? `The player reported: ${ended.message}.` : "Playback stopped with an error.";
        } else if (upNext) {
          // Already counting down over the credits: carry on, in the full panel.
          if (countdown !== null && upNextAt === "credits") {
            upNextAt = "end";
            tick().then(() => playNowButton?.focus());
          } else beginCountdown("end");
        } else {
          void close();
        }
      }),
    ];
    // Untracked as a whole: start() reads state (the quality) before its first await, and a
    // tracked read would re-run this effect on a quality change, tearing the screen down.
    untrack(() => void start(itemId));
    return () => {
      disposed = true;
      for (const listener of listeners) listener.then((unlisten) => unlisten());
      clearTimeout(seekTimer);
      clearInterval(countdownTimer);
      // Left some other way than close() (the session ended): still stop and restore the window.
      if (!closing) {
        void api.stopPlayback().catch(() => {});
        if (mode === "fullscreen") void api.setFullscreen(false).catch(() => {});
      }
    };
  });

  // The server's live updates, laid over what this screen loaded: the playing item's watched
  // mark and favourite, and the list's entries.
  $effect(() => {
    if (detail) {
      const own = live.get(detail.id);
      if (own) {
        favorite = own.favorite;
        played = own.played;
      }
    }
    const entries = [...(seasonList?.entries ?? []), ...(seasonList?.next?.entries ?? []), ...(queue?.entries ?? [])];
    for (const entry of entries) {
      const change = live.get(entry.id);
      if (!change) continue;
      entry.played = change.played;
      entry.progress = change.played ? null : change.progress;
    }
  });

  // --- up next

  const COUNTDOWN_SECONDS = 10;
  let countdownTimer: ReturnType<typeof setInterval> | undefined;
  let playNowButton = $state<HTMLButtonElement>();

  function beginCountdown(at: "credits" | "end") {
    upNextAt = at;
    countdown = COUNTDOWN_SECONDS;
    clearInterval(countdownTimer);
    if (autoplay) {
      countdownTimer = setInterval(() => {
        // Paused during the credits, the countdown waits as well.
        if (countdown === null || (upNextAt === "credits" && paused)) return;
        countdown -= 1;
        if (countdown <= 0) playNext();
      }, 1000);
    }
    // Over the credits the picture is still playing, so the keyboard stays with the player.
    if (at === "end") tick().then(() => playNowButton?.focus());
  }

  // Up next appears as the closing credits start, unless Watch credits was chosen. Seeking back
  // out of the credits takes it away again.
  $effect(() => {
    const at = creditsStart;
    const inCredits = at !== null && started && !error && !!upNext && position >= at;
    untrack(() => {
      if (inCredits && countdown === null && !creditsDismissed) {
        beginCountdown("credits");
      } else if (!inCredits && countdown !== null && upNextAt === "credits") {
        clearInterval(countdownTimer);
        countdown = null;
      }
    });
  });

  function playNext() {
    if (upNext) void start(upNext.itemId);
  }

  /** Cancel after the end leaves the screen; Watch credits keeps playing. */
  function cancelCountdown() {
    clearInterval(countdownTimer);
    countdown = null;
    if (upNextAt === "credits") {
      creditsDismissed = true;
      upNextAt = "end";
    } else {
      void close();
    }
  }

  async function close() {
    if (closing) return;
    closing = true;
    if (mode === "fullscreen") await api.setFullscreen(false).catch(() => {});
    try {
      await api.stopPlayback();
    } catch {
      // Already stopped; nothing to wait for.
    }
    onClose();
  }

  function run(call: Promise<unknown>) {
    call.catch(() => {});
    wake();
  }

  function togglePause() {
    if (started) run(api.togglePause());
  }

  function seekTo(seconds: number) {
    if (!started || duration == null) return;
    const target = clamp(seconds, 0, Math.max(0, duration - 0.5));
    seekTarget = target;
    clearTimeout(seekTimer);
    seekTimer = setTimeout(() => (seekTarget = null), 3000);
    run(api.seek(target));
  }

  const seekBy = (delta: number) => seekTo(position + delta);

  function setSpeed(value: number) {
    if (started) run(api.setSpeed(value));
  }

  /** One step slower or faster through the menu's speeds. */
  function stepSpeed(direction: 1 | -1) {
    const here = SPEEDS.findIndex((s) => Math.abs(s - speed) < 0.01);
    const from = here < 0 ? SPEEDS.indexOf(1) : here;
    setSpeed(SPEEDS[clamp(from + direction, 0, SPEEDS.length - 1)]);
  }

  /** The thumbnail for a moment of the file. */
  function thumbAt(seconds: number) {
    const t = trickplay;
    if (!t || t.itemId !== now?.itemId) return null;
    const index = clamp(Math.floor((seconds * 1000) / t.intervalMs), 0, t.count - 1);
    return {
      url: api.trickplayThumbUrl(t, index),
      w: THUMB_WIDTH,
      h: Math.round((THUMB_WIDTH * t.height) / t.width),
    };
  }

  /** A thumbnail stays inside the seek bar's ends. */
  function tipLeft(x: number, width: number | null) {
    if (!width || !trackBar) return x;
    const half = width / 2 + 4;
    return clamp(x, half, Math.max(half, trackBar.getBoundingClientRect().width - half));
  }

  function setVolume(value: number) {
    run(api.setVolume(clamp(value, 0, 100)));
    if (muted && value > 0) run(api.setMuted(false));
  }

  const toggleMute = () => run(api.setMuted(!muted));

  function toggleSubtitles() {
    const current = player?.subtitleTrack ?? null;
    // Through chooseTrack, so a subtitle drawn in by the server asks for the stream again.
    if (current !== null) {
      lastSubtitle = current;
      chooseTrack("subtitle", null);
      return;
    }
    const pick =
      lastSubtitle ??
      (subtitleTracks.find((t) => t.default && !t.forced) ?? subtitleTracks.find((t) => !t.forced) ?? subtitleTracks[0])?.id;
    if (pick != null) chooseTrack("subtitle", pick);
  }

  function skipLabel(segment: api.Segment): string {
    switch (segment.kind) {
      case "intro":
        return "Skip intro";
      case "recap":
        return "Skip recap";
      case "preview":
        return "Skip preview";
      case "credits":
        return upNext ? "Next episode" : "Skip credits";
    }
  }

  /** Past the segment. Credits with an episode after them go straight to it, as a streaming service does. */
  function skipSegment() {
    const segment = skippable;
    if (!segment) return;
    if (segment.kind === "credits" && upNext) playNext();
    else seekTo(segment.endSeconds);
  }

  /** The next chapter's start, or back to this chapter's start (the one before, right after it began). */
  function jumpChapter(direction: 1 | -1) {
    if (!hasChapters || !started) return;
    const target =
      direction > 0
        ? chapters.find((c) => c.startSeconds > position + 1)
        : lastChapterBefore(chapters, position - 2, false);
    if (target) seekTo(target.startSeconds);
    else if (direction < 0) seekTo(0);
    else if (duration != null) seekTo(duration);
  }

  // --- scrubbing

  function pointAt(clientX: number) {
    if (!trackBar || !duration) return { x: 0, time: 0 };
    const r = trackBar.getBoundingClientRect();
    const x = clamp(clientX - r.left, 0, r.width);
    return { x, time: (x / r.width) * duration };
  }

  function scrubDown(e: PointerEvent) {
    if (!started || !duration || e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const point = pointAt(e.clientX);
    dragging = true;
    dragTime = point.time;
    hover = point;
  }

  function scrubMove(e: PointerEvent) {
    const point = pointAt(e.clientX);
    hover = point;
    if (dragging) dragTime = point.time;
  }

  function scrubUp(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    seekTo(pointAt(e.clientX).time);
  }

  function scrubKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 30 : 5;
    if (e.key === "ArrowLeft") seekBy(-step);
    else if (e.key === "ArrowRight") seekBy(step);
    else if (e.key === "Home") seekTo(0);
    else if (e.key === "End" && duration) seekTo(duration);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  // --- chrome

  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  function wake() {
    awake = true;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => (awake = false), 2500);
  }
  $effect(() => {
    wake();
    return () => clearTimeout(idleTimer);
  });

  $effect(() => {
    if (!menuOpen) return;
    const onPointer = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!menu?.contains(target) && !menuButton?.contains(target)) menuOpen = false;
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  });

  function onKey(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "Escape") {
      if (menuOpen) {
        menuOpen = false;
        menuButton?.focus();
      } else if (countdown !== null) cancelCountdown();
      else if (mode === "fullscreen") toggleFullscreen();
      else if (mode === "theater") void setMode("default");
      else return;
      e.preventDefault();
      return;
    }
    const target = e.target as HTMLElement | null;
    // Fields, menus and lists own their keys (the volume slider its arrows).
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target?.isContentEditable) return;
    if (target instanceof Element && target.closest('[role="menu"], [role="listbox"]')) return;
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    switch (key) {
      case " ":
        if (target instanceof HTMLButtonElement) return;
        togglePause();
        break;
      case "k":
        togglePause();
        break;
      case "j":
      case "ArrowLeft":
        seekBy(-10);
        break;
      case "l":
      case "ArrowRight":
        seekBy(10);
        break;
      case "ArrowUp":
        setVolume(volume + 5);
        break;
      case "ArrowDown":
        setVolume(volume - 5);
        break;
      case "m":
        toggleMute();
        break;
      case "c":
        toggleSubtitles();
        break;
      case "n":
        if (!e.shiftKey || !upNext) return;
        playNext();
        break;
      case "[":
        if (!hasChapters) return;
        jumpChapter(-1);
        break;
      case "]":
        if (!hasChapters) return;
        jumpChapter(1);
        break;
      case "s":
        if (!skippable) return;
        skipSegment();
        break;
      case "<":
        stepSpeed(-1);
        break;
      case ">":
        stepSpeed(1);
        break;
      case "t":
        toggleTheater();
        break;
      case "f":
        toggleFullscreen();
        break;
      case "Home":
        seekTo(0);
        break;
      default:
        if (/^[0-9]$/.test(key) && duration) {
          seekTo((Number(key) / 10) * duration);
          break;
        }
        return;
    }
    e.preventDefault();
    wake();
  }
</script>

<svelte:window onkeydown={onKey} onpointermove={wake} bind:innerWidth />

{#snippet queueItem(entry: api.QueueEntry)}
  {@const current = entry.id === now?.itemId}
  <button
    class="qitem"
    class:is-current={current}
    aria-current={current ? "true" : undefined}
    bind:this={listAnchors[entry.id]}
    onpointerenter={(e) => listHover.enter(entry.id, e)}
    onpointerleave={listHover.leave}
    onclick={() => playEntry(entry.id)}
  >
    <span class="qart">
      <Art image={entry.image} title={entry.title} sub={entry.meta} width={148} />
      {#if current}
        <span class="qnow">Now playing</span>
      {:else if entry.runtimeMinutes}
        <span class="qdur">{runtime(entry.runtimeMinutes * 60)}</span>
      {/if}
      {#if entry.progress && !current}<span class="qprog"><span style:width="{Math.round(entry.progress * 100)}%"></span></span>{/if}
    </span>
    <span class="qbody">
      <span class="qtitle">{entry.title}</span>
      {#if entry.meta || entry.played}
        <span class="qmeta">
          {entry.meta ?? ""}
          {#if entry.played}<span class="qdone"><Icon name="check" size={12} />Watched</span>{/if}
        </span>
      {/if}
    </span>
  </button>
  {#if listHover.key === entry.id && listAnchors[entry.id]}
    <HoverPanel
      anchor={listAnchors[entry.id]}
      label={entry.title}
      onEnter={(e) => listHover.enter(entry.id, e)}
      onLeave={listHover.leave}
      onClose={listHover.close}
      onActivate={current ? undefined : () => playEntry(entry.id)}
    >
      {#if isEpisode}
        <EpisodeHover
          seriesTitle={detail?.series?.title ?? null}
          code={entry.meta}
          title={entry.title}
          premiereDate={entry.premiereDate}
          overview={entry.overview}
          played={entry.played}
          resume={!!entry.progress}
          playing={current}
          onPlay={() => playEntry(entry.id)}
          onTogglePlayed={() => toggleEntryPlayed(entry)}
        />
      {:else}
        <TitleHover id={entry.id} title={entry.title} onPlay={playEntry} />
      {/if}
    </HoverPanel>
  {/if}
{/snippet}

{#snippet listSkeleton()}
  <div class="queue skel" aria-busy="true" aria-label="Loading">
    {#each [0, 1, 2, 3] as i (i)}<div class="qitem is-loading"><span class="qart"></span><span class="qbody"><span></span><span></span></span></div>{/each}
  </div>
{/snippet}

{#snippet listHead()}
  {#if isEpisode && seasonList && seasonOptions.length}
    <Select look="heading" label="Season" value={listSeasonId ?? seasonList.seasonId} options={seasonOptions} onChange={chooseSeason} />
  {:else}
    <h2 class="list-title">{isEpisode ? "Episodes" : (queue?.heading ?? "More like this")}</h2>
  {/if}
  {#if isEpisode && detail?.series}<span class="eyebrow">{detail.series.title}</span>{/if}
{/snippet}

{#snippet listBody(inCard: boolean)}
  {#if isEpisode}
    {#if !seasonList}
      {@render listSkeleton()}
    {:else}
      <div class="season-block" aria-busy={listSeasonId !== seasonList.seasonId}>
        {#if seasonList.entries.length}
          <div class="queue" class:is-grid={inCard}>
            {#each seasonList.entries as entry (entry.id)}{@render queueItem(entry)}{/each}
          </div>
        {:else}
          <p class="queue-empty">Couldn't load this season's episodes.</p>
        {/if}
        {#if seasonList.next}
          {@const next = seasonList.next}
          <!-- Where the list steps into the next season. -->
          <div class="season-divider">{next.name}</div>
          <div class="queue" class:is-grid={inCard}>
            {#each next.entries as entry (entry.id)}{@render queueItem(entry)}{/each}
          </div>
          <button class="btn season-more" onclick={() => chooseSeason(next.seasonId)}>
            All of {next.name}<Icon name="chevr" size={12} />
          </button>
        {/if}
      </div>
    {/if}
  {:else if !queue}
    {@render listSkeleton()}
  {:else if !queue.entries.length}
    <p class="queue-empty">The server found nothing similar in the library.</p>
  {:else}
    <div class="queue" class:is-grid={inCard}>
      {#each queue.entries as entry (entry.id)}{@render queueItem(entry)}{/each}
    </div>
  {/if}
{/snippet}

<div
  class="watch"
  class:is-theater={mode === "theater"}
  class:is-fullscreen={mode === "fullscreen"}
  class:has-side={sideBySide}
  bind:this={scroller}
>
 <div class="main">
  <div class="player-area">
    <!-- Unpainted: mpv draws the picture exactly under this box (see syncViewport). -->
    <div class="player" class:is-idle={!showChrome} bind:this={box} data-morph-target>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="surface" onclick={togglePause} ondblclick={toggleFullscreen}></div>

      {#if error}
        <div class="panel" role="alert">
          <p class="panel-title">Couldn't play this</p>
          <p class="panel-body">{error}</p>
          <button class="btn" onclick={close}>Back to Home</button>
        </div>
      {:else if waiting}
        <div class="buffering">
          <LevelMeter />
          <span class="sr-only" role="status">{started ? "Buffering" : "Loading"}</span>
        </div>
      {:else if paused && (countdown === null || upNextAt === "credits")}
        <span class="paused-mark" aria-hidden="true"><Icon name="play" size={40} /></span>
      {/if}

      {#if countdown !== null && upNext}
        <div
          class="up-next"
          class:is-credits={upNextAt === "credits"}
          class:is-paused={upNextAt === "credits" && paused}
          role={upNextAt === "credits" ? "region" : "dialog"}
          aria-label="Up next"
        >
          <div class="up-next-art">
            <Art image={upNext.image} title={upNext.title} sub={upNext.subtitle} width={220} />
          </div>
          <div class="up-next-body">
            {#if autoplay}
              <span class="up-next-label" role="timer">Up next in {countdown}</span>
            {:else}
              <span class="up-next-label">Up next</span>
            {/if}
            <span class="up-next-title">{upNext.title}</span>
            {#if upNext.subtitle}<span class="up-next-subtitle">{upNext.subtitle}</span>{/if}
            {#if autoplay}
              <span class="up-next-bar" style:--seconds="{COUNTDOWN_SECONDS}s" aria-hidden="true"><span></span></span>
            {/if}
            <div class="up-next-actions">
              <button class="btn btn-primary" bind:this={playNowButton} onclick={playNext}>
                <Icon name="play" />Play now
              </button>
              <button class="btn" onclick={cancelCountdown}>{upNextAt === "credits" ? "Watch credits" : "Cancel"}</button>
            </div>
          </div>
        </div>
      {/if}

      {#if skippable}
        <button class="skip" class:is-low={!showChrome} aria-keyshortcuts="S" onclick={skipSegment}>
          {skipLabel(skippable)}<Icon name="next" />
        </button>
      {/if}

      <div class="chrome top">
        <button class="pbtn" aria-label="Back to Home" onclick={close} disabled={closing}>
          <Icon name="sym:arrow_back" />
        </button>
        {#if now && mode !== "default"}
          <div class="titles">
            <span class="top-title">{now.title}</span>
            {#if now.subtitle}<span class="top-subtitle">{now.subtitle}</span>{/if}
          </div>
        {/if}
      </div>

      <div class="chrome controls" hidden={countdown !== null && upNextAt === "end"}>
        <div
          class="scrub"
          class:is-dragging={dragging}
          role="slider"
          tabindex="0"
          aria-label="Seek"
          aria-valuemin={0}
          aria-valuemax={Math.round(duration ?? 0)}
          aria-valuenow={Math.round(shown)}
          aria-valuetext={`${clock(shown)} of ${clock(duration)}`}
          onpointerdown={scrubDown}
          onpointermove={scrubMove}
          onpointerup={scrubUp}
          onpointercancel={() => (dragging = false)}
          onpointerleave={() => {
            if (!dragging) hover = null;
          }}
          onkeydown={scrubKey}
        >
          <div class="track" bind:this={trackBar}>
            <div class="buffered" style:width="{bufferedPct}%"></div>
            <div class="played" style:width="{playedPct}%"></div>
            {#if hasChapters && duration}
              {#each chapters as chapter, i (i)}
                {#if chapter.startSeconds > 1}<span class="tick" style:left="{(chapter.startSeconds / duration) * 100}%"></span>{/if}
              {/each}
            {/if}
            <div class="thumb" style:left="{playedPct}%"></div>
          </div>
          {#if hover && duration}
            {@const at = dragging ? dragTime : hover.time}
            {@const thumb = thumbAt(at)}
            <span class="scrub-tip" class:has-thumb={!!thumb} style:left="{tipLeft(hover.x, thumb?.w ?? null)}px">
              {#if thumb}
                <span
                  class="tip-thumb"
                  style:width="{thumb.w}px"
                  style:height="{thumb.h}px"
                  style:background-image="url('{thumb.url}')"
                  style:background-size="100% 100%"
                ></span>
              {/if}
              <span class="tip-text">{clock(at)}{#if hoverChapter?.name}<span class="tip-chapter">{hoverChapter.name}</span>{/if}</span>
            </span>
          {/if}
        </div>

        <div class="ctrl-row">
          {#if hasChapters}
            <button class="pbtn" aria-label="Previous chapter ([)" disabled={!started} onclick={() => jumpChapter(-1)}>
              <Icon name="prev" />
            </button>
          {/if}
          <button class="pbtn pbtn-main" aria-label={paused ? "Play (K)" : "Pause (K)"} disabled={!started} onclick={togglePause}>
            <Icon name={paused ? "play" : "pause"} />
          </button>
          {#if hasChapters}
            <button class="pbtn" aria-label="Next chapter (])" disabled={!started} onclick={() => jumpChapter(1)}>
              <Icon name="next" />
            </button>
          {/if}
          {#if upNext}
            <button
              class="pbtn"
              aria-label={`Next episode, ${upNext.subtitle ?? upNext.title} (Shift+N)`}
              title={upNext.subtitle ?? upNext.title}
              onclick={playNext}
            >
              <Icon name="next-episode" />
            </button>
          {/if}
          <div class="vol">
            <button class="pbtn mute" aria-label={muted ? "Unmute (M)" : "Mute (M)"} style:--vol={muted ? 0 : volume / 100} onclick={toggleMute}>
              <Icon name={muted ? "mute" : "vol"} />
            </button>
            <span class="vol-track">
              <input
                type="range"
                min="0"
                max="100"
                step="1"
                value={Math.round(muted ? 0 : volume)}
                style:--pct="{Math.round(muted ? 0 : volume)}%"
                aria-label="Volume"
                oninput={(e) => setVolume(Number(e.currentTarget.value))}
              />
            </span>
          </div>
          <span class="time">{clock(shown)} / {clock(duration)}</span>
          {#if Math.abs(speed - 1) > 0.01}<span class="speed-now">{speedLabel(speed)}</span>{/if}

          <div class="right">
            <button
              class="pbtn"
              aria-label="Subtitles (C)"
              aria-pressed={player?.subtitleTrack != null}
              disabled={!subtitleTracks.length}
              onclick={toggleSubtitles}
            >
              <Icon name="cc" />
            </button>
            <button
              class="pbtn"
              bind:this={menuButton}
              aria-label="Quality, speed, audio and subtitles"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              disabled={!started && !tracks.length && !qualityOptions.length}
              onclick={() => (menuOpen = !menuOpen)}
            >
              <Icon name="gear" />
            </button>
            <button
              class="pbtn"
              aria-label="Theater mode (T)"
              aria-pressed={mode === "theater"}
              disabled={mode === "fullscreen"}
              onclick={toggleTheater}
            >
              <Icon name="theater" />
            </button>
            <button
              class="pbtn"
              aria-label={mode === "fullscreen" ? "Exit full screen (F)" : "Full screen (F)"}
              aria-pressed={mode === "fullscreen"}
              onclick={toggleFullscreen}
            >
              <Icon name="expand" />
            </button>
          </div>
        </div>
      </div>

      {#if menuOpen}
        <div class="popover" role="menu" aria-label="Quality, speed, audio and subtitles" bind:this={menu}>
          {#if qualityOptions.length}
            <div class="pop-sec">
              <div class="pop-label">Quality</div>
              <button
                class="pop-opt"
                role="menuitemradio"
                aria-checked={quality === "original"}
                onclick={() => chooseQuality("original")}
              >
                <span class="opt-text">
                  <span class="opt-name">Original</span>
                  {#if sourceLabel}<small>{sourceLabel}</small>{/if}
                </span>
                <span class="mark"><Icon name="check" /></span>
              </button>
              {#each qualityOptions as option (option.id)}
                <button
                  class="pop-opt"
                  role="menuitemradio"
                  aria-checked={quality === option.id}
                  onclick={() => chooseQuality(option.id)}
                >
                  <span class="opt-text">
                    <span class="opt-name">{option.id}</span>
                    <small>Transcoded by the server, up to {option.bitrate / 1e6} Mb/s</small>
                  </span>
                  <span class="mark"><Icon name="check" /></span>
                </button>
              {/each}
            </div>
          {/if}
          <div class="pop-sec">
            <div class="pop-label">Speed</div>
            <div class="speeds" role="group" aria-label="Playback speed">
              {#each SPEEDS as option (option)}
                <button
                  class="speed"
                  role="menuitemradio"
                  aria-checked={Math.abs(speed - option) < 0.01}
                  disabled={!started}
                  onclick={() => setSpeed(option)}
                >
                  {speedLabel(option)}
                </button>
              {/each}
            </div>
          </div>
          <div class="pop-sec">
            <div class="pop-label">Audio</div>
            {#each audioTracks as track, i (track.id)}
              <button
                class="pop-opt"
                role="menuitemradio"
                aria-checked={player?.audioTrack === track.id}
                onclick={() => chooseTrack("audio", track.id)}
              >
                <span class="opt-text">
                  <span class="opt-name">{trackName(track, i)}</span>
                  {#if trackDetail(track)}<small>{trackDetail(track)}</small>{/if}
                </span>
                <span class="mark"><Icon name="check" /></span>
              </button>
            {:else}
              <p class="pop-empty">This file has no audio.</p>
            {/each}
          </div>
          <div class="pop-sec">
            <div class="pop-label">Subtitles</div>
            <button
              class="pop-opt"
              role="menuitemradio"
              aria-checked={player?.subtitleTrack == null}
              onclick={() => chooseTrack("subtitle", null)}
            >
              <span class="opt-text"><span class="opt-name">Off</span></span>
              <span class="mark"><Icon name="check" /></span>
            </button>
            {#each subtitleTracks as track, i (track.id)}
              <button
                class="pop-opt"
                role="menuitemradio"
                aria-checked={player?.subtitleTrack === track.id}
                onclick={() => chooseTrack("subtitle", track.id)}
              >
                <span class="opt-text">
                  <span class="opt-name">{trackName(track, i)}</span>
                  {#if trackDetail(track)}<small>{trackDetail(track)}</small>{/if}
                </span>
                <span class="mark"><Icon name="check" /></span>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>

  {#if mode !== "fullscreen"}
    <div class="details">
      {#if now}
        <div class="details-inner">
          <div class="readout">
            <span class="readout-state" class:is-transcoding={now.transcoding}>{now.method}</span>
            {#if readout.length}
              <span class="readout-data">
                {#each readout as part (part)}<span>{part}</span>{/each}
              </span>
            {/if}
            {#if started}<span class="readout-dec">{decoderLabel(player?.decoder)}</span>{/if}
          </div>

          <div class="title-block">
            <h1 class="title">{now.title}</h1>
            <div class="meta-line">
              {#each meta as part, i (i)}
                {#if i > 0}<span class="sep"></span>{/if}<span>{part}</span>
              {/each}
              {#if detail?.officialRating}<span class="tag">{detail.officialRating}</span>{/if}
            </div>
            {#if detail}
              <div class="bar-row">
                {#if detail.series}
                  {@const series = detail.series}
                  <button class="owner" onclick={() => onOpenItem(series.id)}>
                    <span class="owner-art"><Art image={series.poster} title={series.title} width={40} /></span>
                    <span class="owner-text">
                      <span class="owner-name">{series.title}</span>
                      {#if detail.seasonName}<span class="owner-sub">{detail.seasonName}</span>{/if}
                    </span>
                  </button>
                {/if}
                {#if !isTrailer}
                <div class="actions">
                  <button class="btn" aria-pressed={played} disabled={acting} onclick={() => toggle("played")}>
                    <Icon name="check" />{played ? "Watched" : "Mark watched"}
                  </button>
                  <button class="btn" aria-pressed={favorite} disabled={acting} onclick={() => toggle("favorite")}>
                    <Icon name="heart" size={14} />{favorite ? "Favorited" : "Favorite"}
                  </button>
                  <DownloadButton itemId={detail.id} iconSize={14} {onOpenDownloads} onError={downloadFailed} />
                </div>
                {/if}
              </div>
              {#if actionError}<p class="action-error" role="alert">{actionError}</p>{/if}
            {/if}
          </div>

          {#if !isTrailer && (now.overview || detail?.media)}
            <section class="card">
              <div class="card-head"><h2 class="card-title">Media info</h2></div>
              {#if now.overview}<p class="synopsis">{now.overview}</p>{/if}
              {#if detail?.media}
                <div class="card-specs" class:has-rule={!!now.overview}>
                  <MediaSpecs media={detail.media} studios={detail.studios} />
                </div>
              {/if}
            </section>
          {/if}

          {#if detail?.people.length}
            <section class="card">
              <div class="card-head">
                <h2 class="card-title">Cast</h2>
                <span class="eyebrow">{detail.people.length} {detail.people.length === 1 ? "person" : "people"}</span>
              </div>
              <div class="cast">
                {#each detail.people as person, i (`${person.id}-${i}`)}
                  <div class="cast-item">
                    <span class="cast-art"><Art image={person.image} title={person.name} width={104} /></span>
                    <span class="cast-name">{person.name}</span>
                    {#if person.role}<span class="cast-role">{person.role}</span>{/if}
                  </div>
                {/each}
              </div>
            </section>
          {/if}

          {#if !sideBySide && (isEpisode ? seasonList?.entries.length : queue?.entries.length)}
            <section class="card">
              <div class="card-head">{@render listHead()}</div>
              {@render listBody(true)}
            </section>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
 </div>

  {#if sideBySide}
    <aside class="side" aria-label={isEpisode ? "Episodes" : "More like this"} bind:this={side}>
      <div class="side-head">{@render listHead()}</div>
      {@render listBody(false)}
    </aside>
  {/if}
</div>

<style>
  /* The whole screen scrolls, the details passing over the pinned player. The ground is the
     player box's spread shadow, clipped to this scroller. */
  .watch {
    --player-ground: var(--ground);
    --gutter: clamp(16px, 2.2vw, 26px);
    height: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-items: start;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .main {
    min-width: 0;
  }
  /* The player column (the 1280px box plus its gutters) and the list stay together in the
     middle of a very wide window. */
  .watch.has-side {
    grid-template-columns: minmax(0, 1332px) 352px;
    justify-content: center;
  }
  /* Pinned while the page scrolls, and scrolling on its own: the height is the stage's, the
     window less the 56px top bar. Painted, so nothing under the webview shows through. */
  .side {
    position: sticky;
    top: 0;
    height: calc(100dvh - var(--bar-h, 64px));
    overflow-y: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px var(--gutter) 40px 0;
    background: var(--player-ground);
  }
  .watch.is-theater {
    --player-ground: color-mix(in oklab, var(--ground) 88%, #000);
  }

  /* Pinned: the details slide up over the player rather than the player scrolling away. mpv
     draws the picture under the page, and a picture chasing a scrolling box trails it by a
     frame or two, which reads as a wobble. A box that never moves can't trail. */
  .player-area {
    position: sticky;
    top: 0;
    z-index: 0;
    padding: 16px var(--gutter) 0;
  }
  .player {
    position: relative;
    margin-inline: auto;
    aspect-ratio: 16 / 9;
    max-width: min(100%, 1280px);
    max-height: calc(100dvh - var(--bar-h, 64px) - 216px);
    border-radius: var(--r-lg);
    overflow: hidden;
    isolation: isolate;
    color: #edefee;
    /* Paints the page around the box while leaving the box itself clear for the picture. */
    box-shadow: 0 0 0 200vmax var(--player-ground);
  }
  .watch.is-theater .player-area,
  .watch.is-fullscreen .player-area {
    padding: 0;
  }
  .watch.is-theater .player {
    max-width: 100%;
    max-height: calc(100dvh - var(--bar-h, 64px) - 84px);
    border-radius: 0;
  }
  .watch.is-fullscreen .player {
    position: fixed;
    inset: 0;
    z-index: 20;
    width: 100vw;
    height: 100vh;
    max-width: none;
    max-height: none;
    aspect-ratio: auto;
    border-radius: 0;
    box-shadow: none;
  }

  .surface {
    position: absolute;
    inset: 0;
    cursor: pointer;
  }
  .player.is-idle,
  .player.is-idle .surface {
    cursor: none;
  }

  .chrome {
    position: absolute;
    left: 0;
    right: 0;
    z-index: 2;
    transition: opacity 0.25s var(--ease);
  }
  .player.is-idle .chrome {
    opacity: 0;
    pointer-events: none;
  }
  .top {
    top: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px 30px;
    background: linear-gradient(180deg, rgba(12, 14, 13, 0.72), rgba(12, 14, 13, 0));
  }
  .titles {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.25;
  }
  .top-title {
    font-stretch: 118%;
    font-weight: 600;
    letter-spacing: -0.01em;
    font-size: 15px;
  }
  .top-subtitle {
    font-size: 12.5px;
    color: #b9bfbd;
  }
  .top-title,
  .top-subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .controls {
    bottom: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 26px 12px 10px;
    background: linear-gradient(to top, rgba(12, 14, 13, 0.92) 0%, rgba(12, 14, 13, 0.72) 44%, rgba(12, 14, 13, 0) 100%);
  }

  .scrub {
    position: relative;
    height: 16px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
  }
  .scrub:focus-visible {
    outline: none;
  }
  .scrub:focus-visible .track {
    outline: 2px solid var(--md-sys-color-primary);
    outline-offset: 4px;
  }
  .track {
    position: relative;
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: rgba(240, 242, 241, 0.26);
  }
  .scrub:hover .track,
  .scrub.is-dragging .track {
    height: 6px;
  }
  .track,
  .buffered,
  .played {
    transition: height 0.15s var(--ease);
  }
  .buffered,
  .played {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 2px;
  }
  .buffered {
    background: rgba(240, 242, 241, 0.4);
  }
  .played {
    background: var(--md-sys-color-primary);
  }
  .thumb {
    position: absolute;
    top: 50%;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--md-sys-color-primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--md-sys-color-primary) 24%, transparent);
    transform: translate(-50%, -50%) scale(0);
    transition: transform var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized);
  }
  .scrub:hover .thumb,
  .scrub.is-dragging .thumb,
  .scrub:focus-visible .thumb {
    transform: translate(-50%, -50%) scale(1);
  }
  .scrub-tip {
    position: absolute;
    bottom: 20px;
    transform: translateX(-50%);
    padding: 4px 8px;
    border-radius: var(--md-sys-shape-sm);
    background: var(--md-sys-color-inverse-surface);
    color: var(--md-sys-color-inverse-on-surface);
    box-shadow: var(--md-sys-elevation-2);
    font: 500 12px/16px var(--f-ui);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }

  /* With the server's thumbnails: the picture at that moment above the time. */
  .scrub-tip.has-thumb {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px;
  }
  .tip-thumb {
    display: block;
    border-radius: 4px;
    background-color: #0c0e0d;
    background-repeat: no-repeat;
  }
  .tip-text {
    padding: 0 3px 1px;
  }

  .ctrl-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  /* Only there when the speed isn't normal, so it's never forgotten on. */
  .speed-now {
    margin-left: 8px;
    padding: 1px 6px;
    border: 1px solid rgba(255, 255, 255, 0.28);
    border-radius: var(--md-sys-shape-sm);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .speeds {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px 10px 8px;
  }
  .speed {
    min-width: 48px;
    height: 32px;
    padding: 0 8px;
    border: 1px solid var(--md-sys-color-outline-variant);
    border-radius: var(--md-sys-shape-sm);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    font: 500 13px/16px var(--f-ui);
    transition: background-color var(--md-sys-motion-duration-short) var(--md-sys-motion-standard);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .speed:hover,
  .speed:focus-visible {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .speed[aria-checked="true"] {
    border-color: transparent;
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  .right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .pbtn {
    position: relative;
    overflow: hidden;
    width: 40px;
    height: 40px;
    flex: none;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: var(--md-sys-shape-full);
    background: none;
    color: inherit;
    cursor: pointer;
    transition:
      background-color var(--md-sys-motion-duration-short) var(--md-sys-motion-standard),
      color var(--md-sys-motion-duration-short) var(--md-sys-motion-standard),
      transform var(--md-sys-motion-duration-short) var(--md-sys-motion-emphasized);
  }
  .pbtn:hover:not([disabled]) {
    background: rgba(240, 242, 241, 0.16);
  }
  .pbtn:active:not([disabled]) {
    transform: scale(0.92);
  }
  .pbtn[aria-pressed="true"] {
    color: var(--md-sys-color-primary);
  }
  /* The one that matters: a filled circle. */
  .pbtn-main {
    width: 48px;
    height: 48px;
    margin-inline: 4px;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  .pbtn-main:hover:not([disabled]) {
    background: color-mix(in srgb, var(--md-sys-color-primary) 88%, white);
  }
  .pbtn[disabled] {
    opacity: 0.45;
    cursor: default;
  }
  .time {
    padding-inline: 8px;
    font-size: 12px;
    letter-spacing: 0.02em;
    color: #d8dbda;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .vol {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .vol-track {
    width: 0;
    overflow: hidden;
    transition: width 0.25s var(--ease);
  }
  .vol:hover .vol-track,
  .vol:focus-within .vol-track {
    width: 72px;
  }
  .vol-track input {
    width: 72px;
    height: 20px;
    margin: 0;
    background: transparent;
    appearance: none;
    -webkit-appearance: none;
    cursor: pointer;
  }
  .vol-track input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--md-sys-color-primary) var(--pct, 100%), rgba(240, 242, 241, 0.3) var(--pct, 100%));
  }
  .vol-track input::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -5px;
    border-radius: 50%;
    background: var(--md-sys-color-primary);
    border: 0;
  }
  .vol-track input::-moz-range-track {
    height: 4px;
    border-radius: 2px;
    background: rgba(240, 242, 241, 0.3);
  }
  .vol-track input::-moz-range-progress {
    height: 4px;
    border-radius: 2px;
    background: var(--md-sys-color-primary);
  }
  .vol-track input::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border: 0;
    border-radius: 50%;
    background: var(--md-sys-color-primary);
  }
  /* The volume icon's bars follow the volume: the icon is the same instrument as the meter. */
  .mute :global(.lv) {
    transform-box: fill-box;
    transform-origin: 50% 50%;
    transition: transform 0.16s var(--ease-mech);
  }
  .mute :global(.lv1) {
    transform: scaleY(clamp(0.25, calc(var(--vol, 1) * 1.9), 1));
  }
  .mute :global(.lv2) {
    transform: scaleY(clamp(0.12, calc((var(--vol, 1) - 0.4) * 1.7), 1));
  }

  /* Buffering: a level meter. Irregular per-bar timing so it reads as a meter, not a wave. */
  .buffering {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }

  .paused-mark {
    position: absolute;
    left: 50%;
    top: 50%;
    z-index: 1;
    width: 80px;
    height: 80px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    box-shadow: var(--md-sys-elevation-3);
    transform: translate(-50%, -50%);
    pointer-events: none;
    animation: mark-in var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized-decelerate) both;
  }

  @keyframes mark-in {
    from {
      opacity: 0;
      transform: translate(-50%, -50%) scale(0.6);
    }
    to {
      opacity: 1;
      transform: translate(-50%, -50%);
    }
  }

  .panel {
    position: absolute;
    left: 50%;
    top: 50%;
    z-index: 3;
    transform: translate(-50%, -50%);
    max-width: min(440px, calc(100% - 32px));
    padding: 24px;
    border-radius: var(--md-sys-shape-xl);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
    box-shadow: var(--md-sys-elevation-3);
  }
  .panel-title {
    margin: 0 0 8px;
    font: 400 24px/32px var(--f-ui);
  }
  .panel-body {
    margin: 0 0 16px;
    font: 400 14px/20px var(--f-ui);
    letter-spacing: 0.25px;
    color: var(--md-sys-color-on-surface-variant);
  }

  .popover {
    position: absolute;
    right: 12px;
    bottom: 64px;
    z-index: 3;
    width: 290px;
    max-height: calc(100% - 84px);
    overflow-y: auto;
    padding: 8px 0;
    color: var(--md-sys-color-on-surface);
    background: var(--md-sys-color-surface-container);
    border-radius: var(--md-sys-shape-lg);
    box-shadow: var(--md-sys-elevation-3);
    transform-origin: bottom right;
    animation: pop var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized-decelerate) both;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.9) translateY(8px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  .pop-sec + .pop-sec {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--md-sys-color-outline-variant);
  }
  .pop-label {
    padding: 8px 16px 4px;
    font: 500 12px/16px var(--f-ui);
    letter-spacing: 0.5px;
    color: var(--md-sys-color-primary);
  }
  .pop-opt {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    min-height: 48px;
    padding: 4px 16px;
    border: 0;
    background: none;
    color: var(--md-sys-color-on-surface);
    font: 400 14px/20px var(--f-ui);
    letter-spacing: 0.1px;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--md-sys-motion-duration-short) var(--md-sys-motion-standard);
  }
  .pop-opt:hover,
  .pop-opt:focus-visible {
    outline: none;
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 10%, transparent);
  }
  .opt-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .opt-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .opt-text small {
    font-size: 11.5px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .mark {
    display: grid;
    flex: none;
    color: var(--md-sys-color-primary);
    opacity: 0;
  }
  .pop-opt[aria-checked="true"] .mark {
    opacity: 1;
  }
  .pop-empty {
    margin: 0;
    padding: 6px 8px;
    font-size: 13px;
    color: var(--ink-3);
  }

  /* Bottom right of the picture, above the control bar, and still there when the controls hide,
     when it settles lower. Solid dark like the other panels on video, whatever the theme. */
  .skip {
    position: absolute;
    right: 16px;
    bottom: 76px;
    z-index: 3;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 48px;
    padding: 0 20px 0 24px;
    border: 0;
    border-radius: var(--md-sys-shape-full);
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    box-shadow: var(--md-sys-elevation-3);
    font: 500 14px/20px var(--f-ui);
    letter-spacing: 0.1px;
    cursor: pointer;
    transition:
      transform var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized),
      box-shadow var(--md-sys-motion-duration-short) var(--md-sys-motion-standard);
  }
  .skip:hover,
  .skip:focus-visible {
    box-shadow: var(--md-sys-elevation-4);
  }
  .skip.is-low {
    transform: translateY(56px);
  }

  .up-next {
    position: absolute;
    left: 50%;
    top: 50%;
    z-index: 4;
    transform: translate(-50%, -50%);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px;
    width: max-content;
    max-width: min(560px, calc(100% - 32px));
    padding: 16px;
    border-radius: var(--md-sys-shape-xl);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
    box-shadow: var(--md-sys-elevation-3);
  }
  .up-next-art {
    position: relative;
    flex: none;
    width: 220px;
    max-width: 100%;
    aspect-ratio: 16 / 9;
  }
  .up-next-art :global(.art) {
    position: absolute;
    inset: 0;
  }
  .up-next-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1 1 220px;
  }
  .up-next-label {
    font: 500 12px/16px var(--f-ui);
    letter-spacing: 0.5px;
    color: var(--md-sys-color-primary);
    font-variant-numeric: tabular-nums;
  }
  .up-next-title {
    font: 500 16px/24px var(--f-ui);
    letter-spacing: 0.15px;
  }
  .up-next-subtitle {
    font: 400 14px/20px var(--f-ui);
    color: var(--md-sys-color-on-surface-variant);
  }
  .up-next-title,
  .up-next-subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The countdown as a bar that drains, so the time left reads at a glance. */
  .up-next-bar {
    position: relative;
    height: 4px;
    margin-block: 8px 6px;
    border-radius: 2px;
    overflow: hidden;
    background: var(--md-sys-color-surface-container-highest);
  }
  .up-next-bar span {
    position: absolute;
    inset: 0;
    background: var(--md-sys-color-primary);
    transform-origin: left center;
    animation: drain var(--seconds) linear forwards;
  }
  @keyframes drain {
    from {
      transform: scaleX(1);
    }
    to {
      transform: scaleX(0);
    }
  }
  .up-next-actions {
    display: flex;
    gap: 8px;
  }
  /* Over the closing credits: a card in the corner above the controls, where the skip button
     sits, with the picture still playing beside it. */
  .up-next.is-credits {
    top: auto;
    left: auto;
    right: 16px;
    bottom: 76px;
    transform: none;
    gap: 12px;
    max-width: min(440px, calc(100% - 32px));
    padding: 12px;
    transition: transform 0.25s var(--ease);
  }
  .player.is-idle .up-next.is-credits {
    transform: translateY(56px);
  }
  .up-next.is-credits .up-next-art {
    width: 140px;
  }
  .up-next.is-credits .up-next-body {
    flex-basis: 200px;
  }
  .up-next.is-paused .up-next-bar span {
    animation-play-state: paused;
  }

  /* Above the pinned player and painted, so scrolling slides it over the picture like a sheet. */
  .details {
    position: relative;
    z-index: 1;
    padding: 14px var(--gutter) 40px;
    background: var(--player-ground);
  }
  .details-inner {
    max-width: 1280px;
    margin-inline: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .readout {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 14px;
    padding: 10px 16px;
    border-radius: var(--md-sys-shape-md);
    background: var(--md-sys-color-surface-container);
    font: 400 12px/16px var(--f-ui);
    letter-spacing: 0.4px;
    color: var(--md-sys-color-on-surface-variant);
    font-variant-numeric: tabular-nums;
  }
  .readout-state {
    color: var(--good);
    font-weight: 500;
  }
  .readout-state.is-transcoding {
    color: var(--alert);
  }
  .readout-data {
    display: flex;
    flex-wrap: wrap;
  }
  .readout-data span {
    padding-inline: 11px;
    border-left: 1px solid var(--line-soft);
  }
  .readout-data span:first-child {
    padding-left: 0;
    border-left: 0;
  }
  .readout-dec {
    margin-left: auto;
    opacity: 0.8;
  }

  .title-block {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .title {
    margin: 0;
    font-weight: 400;
    font-size: clamp(1.75rem, 2.4vw, 2rem);
    line-height: 1.25;
  }
  .meta-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }
  .sep {
    width: 1px;
    height: 11px;
    flex: none;
    background: var(--line);
  }
  .synopsis {
    margin: 0;
    white-space: pre-line;
    max-width: 65ch;
    font-size: 14px;
    line-height: 1.65;
    color: var(--ink-2);
  }
  .tag {
    padding: 1px 6px;
    border: 1px solid var(--line);
    border-radius: var(--md-sys-shape-xs);
    font-size: 11.5px;
    white-space: nowrap;
  }

  /* Chapter starts on the seek bar. */
  .tick {
    position: absolute;
    top: 50%;
    width: 2px;
    height: 9px;
    border-radius: 1px;
    background: rgba(18, 20, 19, 0.55);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }
  .tip-chapter {
    margin-left: 7px;
    color: #b9bfbd;
  }

  .bar-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding-block: 4px;
  }
  .owner {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    margin-left: -6px;
    padding: 4px 10px 4px 6px;
    border: 0;
    border-radius: var(--r);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 0.16s var(--ease);
  }
  .owner:hover {
    background: var(--surface);
  }
  .owner-art {
    position: relative;
    flex: none;
    width: 38px;
    height: 38px;
  }
  .owner-art > :global(.art) {
    position: absolute;
    inset: 0;
    border-radius: 9px;
  }
  .owner-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.25;
  }
  .owner-name {
    font-size: 14px;
    font-weight: 500;
  }
  .owner-sub {
    font-size: 12px;
    color: var(--ink-3);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-left: auto;
  }
  /* Global, so the Download button (its own component) matches the others. */
  .actions :global(.btn) {
    border-color: transparent;
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  .actions :global(.btn[aria-pressed="true"]) {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .action-error {
    margin: 0;
    font-size: 13px;
    color: var(--alert);
  }

  .card {
    padding: 20px 24px;
    border-radius: var(--md-sys-shape-lg);
    background: var(--md-sys-color-surface-container);
  }
  .card-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .card-title {
    margin: 0;
    font: 500 16px/24px var(--f-ui);
    letter-spacing: 0.15px;
  }
  .card-specs.has-rule {
    margin-top: 14px;
    padding-top: 14px;
    border-top: 1px solid var(--line-soft);
  }

  .cast {
    display: flex;
    gap: 10px;
    overflow-x: auto;
    padding-bottom: 4px;
    scrollbar-width: thin;
  }
  .cast-item {
    flex: none;
    width: 104px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .cast-art {
    position: relative;
    width: 104px;
    aspect-ratio: 1;
    margin-bottom: 4px;
  }
  .cast-art > :global(.art) {
    position: absolute;
    inset: 0;
    border-radius: 50%;
  }
  .cast-name {
    font-size: 12.5px;
    font-weight: 500;
    line-height: 1.3;
  }
  .cast-role {
    font-size: 11.5px;
    line-height: 1.3;
    color: var(--ink-3);
  }

  .side-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 34px;
  }
  .list-title {
    margin: 0;
    font: 500 16px/24px var(--f-ui);
    letter-spacing: 0.15px;
  }
  /* The season picker keeps its width; a long show name gives way instead. */
  .side-head .eyebrow {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .qitem.is-current {
    background: var(--md-sys-color-secondary-container);
    cursor: default;
  }
  .qnow {
    position: absolute;
    left: 5px;
    bottom: 5px;
    z-index: 2;
    padding: 2px 8px;
    border-radius: var(--md-sys-shape-sm);
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    font: 500 12px/16px var(--f-ui);
  }
  .qdone {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 8px;
    color: var(--good);
  }
  .season-block[aria-busy="true"] {
    opacity: 0.55;
    transition: opacity 0.2s var(--ease);
  }
  /* Low-key on purpose: a label and a hairline where the next season starts. */
  .season-divider {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 14px 6px 4px;
    font-size: 12px;
    color: var(--ink-3);
  }
  .season-divider::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--line-soft);
  }
  .season-more {
    margin: 8px 6px 0;
  }
  .queue {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .queue.is-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 4px 12px;
  }
  .qitem {
    display: grid;
    grid-template-columns: 148px minmax(0, 1fr);
    align-items: start;
    gap: 10px;
    padding: 8px;
    border: 0;
    border-radius: var(--md-sys-shape-lg);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background-color var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard);
  }
  .qitem:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .qitem.is-current:hover {
    background: var(--md-sys-color-secondary-container);
  }
  .qart {
    position: relative;
    width: 148px;
    aspect-ratio: 16 / 9;
  }
  .qart > :global(.art) {
    position: absolute;
    inset: 0;
  }
  .qdur {
    position: absolute;
    right: 5px;
    bottom: 5px;
    z-index: 2;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(18, 20, 19, 0.84);
    color: #edefee;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .qprog {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 2;
    height: 4px;
    overflow: hidden;
    background: color-mix(in srgb, var(--md-sys-color-surface) 60%, transparent);
  }
  .qprog span {
    display: block;
    height: 100%;
    background: var(--md-sys-color-primary);
  }
  .qbody {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    padding-top: 2px;
  }
  .qtitle {
    font: 500 14px/20px var(--f-ui);
    letter-spacing: 0.1px;
  }
  .qmeta {
    font: 400 12px/16px var(--f-ui);
    letter-spacing: 0.4px;
    color: var(--md-sys-color-on-surface-variant);
    font-variant-numeric: tabular-nums;
  }
  /* Placeholders shaped like the rows; the scan line over them is `.skel` in app.css. */
  .qitem.is-loading {
    cursor: default;
    pointer-events: none;
  }
  .qitem.is-loading .qart {
    border-radius: var(--r);
    background: var(--surface-2);
  }
  .qitem.is-loading .qbody span {
    height: 12px;
    width: 80%;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .qitem.is-loading .qbody span + span {
    width: 45%;
  }
  .queue-empty {
    margin: 0;
    padding: 6px;
    font-size: 13px;
    color: var(--ink-3);
  }

  @media (max-width: 860px) {
    .qitem {
      grid-template-columns: 132px minmax(0, 1fr);
    }
    .qart {
      width: 132px;
    }
  }
</style>
