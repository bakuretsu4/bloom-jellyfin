// Typed wrappers for the Rust commands in src-tauri/src/. The page never sees an access token:
// Rust makes every server request, and artwork arrives through the bloom-img:// scheme.
// A rejected call carries `{ code, message }`; the message is written for people.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { view } from "./zoom.svelte";

export type Server = { address: string; id: string; name: string; version: string };
export type User = { id: string; name: string };
export type PublicUser = User & { hasPassword: boolean };
export type ServerCheck = { server: Server; quickConnect: boolean; users: PublicUser[] };
export type Account = { server: Server; user: User };
export type Startup = { account: Account | null; offline: boolean; lastAddress: string | null };

export type Image = { itemId: string; kind: "Primary" | "Thumb" | "Backdrop" | "Logo"; tag: string };
export type Spotlight = {
  id: string;
  kind: string;
  title: string;
  overview: string | null;
  backdrop: Image;
  logo: Image | null;
  year: number | null;
  officialRating: string | null;
  communityRating: number | null;
  genres: string[];
  runtimeMinutes: number | null;
  seasons: number | null;
  favorite: boolean;
};
export type Shape = "poster" | "wide" | "square";
export type Card = {
  id: string;
  kind: string;
  title: string;
  meta: string | null;
  image: Image | null;
  progress: number | null;
  /** For an episode: its show, whose page the card's text opens. */
  seriesId?: string | null;
};
export type Section = { id: string; title: string; shape: Shape; cards: Card[] };

/** How much picture to ask the server for. Anything but Original may transcode. */
export type Quality = "original" | "1080p" | "720p";
export type SourceVideo = { width: number | null; height: number | null; codec: string | null; bitrate: number | null };
export type NowPlaying = {
  itemId: string;
  /** Jellyfin's item type: "Movie", "Episode" and so on. */
  kind: string;
  title: string;
  subtitle: string | null;
  overview: string | null;
  method: string;
  transcoding: boolean;
  durationSeconds: number | null;
  startSeconds: number;
  quality: Quality;
  /** The file's own picture. */
  source: SourceVideo | null;
  /** A downloaded copy, playing from disk. */
  downloaded: boolean;
};
export type PlayerState = {
  itemId: string;
  position: number;
  duration: number | null;
  paused: boolean;
  buffering: boolean;
  /** The first frame has been shown. */
  started: boolean;
  decoder: string | null;
  /** The media time buffered up to. */
  cacheEnd: number | null;
  volume: number;
  muted: boolean;
  /** 1 is normal. */
  speed: number;
  audioTrack: number | null;
  subtitleTrack: number | null;
};
export type Track = {
  id: number;
  kind: "video" | "audio" | "sub";
  ffIndex: number | null;
  title: string | null;
  lang: string | null;
  codec: string | null;
  channels: number | null;
  width: number | null;
  height: number | null;
  fps: number | null;
  default: boolean;
  forced: boolean;
  external: boolean;
  hearingImpaired: boolean;
  selected: boolean;
};
export type PlayerEnded = { itemId: string; reason: "finished" | "error"; message: string | null };
export type UpNext = { itemId: string; title: string; subtitle: string | null; image: Image | null };

export type Theme = "auto" | "light" | "dark";
export type Accent = "amber" | "green" | "blue" | "red";
/** A theme someone made: four `#rrggbb` colours (src/lib/theme.ts works out the rest). */
export type CustomTheme = {
  id: string;
  name: string;
  base: "light" | "dark";
  ground: string;
  surface: string;
  ink: string;
  accent: string;
  /** Tokens set by hand as CSS, over the ones worked out from the four colours. */
  tokens?: Record<string, string>;
  /** An installed font family; null is Bloom's Archivo. */
  font?: string | null;
};
/** Font families installed on this computer. */
export const systemFonts = () => invoke<string[]>("system_fonts");
export type Hwdec = "auto" | "nvdec" | "vaapi" | "software";
export type SubtitleMode = "server" | "off" | "forced" | "always";
/** Device settings, as src-tauri/src/settings.rs keeps them. */
export type Settings = {
  zoom: number;
  theme: Theme;
  accent: Accent;
  customThemes: CustomTheme[];
  /** The custom theme in use; null uses `theme` and `accent`. */
  customTheme: string | null;
  reduceMotion: boolean;
  hwdec: Hwdec;
  directPlay: boolean;
  maxQuality: Quality;
  /** A language code ("ja"); null leaves it to the server. */
  audioLanguage: string | null;
  subtitleMode: SubtitleMode;
  /** null follows the audio's language. */
  subtitleLanguage: string | null;
  /** Plain and styled subtitles; image subtitles keep the file's size. */
  subtitleSize: "small" | "normal" | "large" | "huge";
  /** Behind plain subtitles only. */
  subtitleBackground: "outline" | "box";
  autoplayNext: boolean;
  /** The player's volume as last set; saved by the player, not the Settings screen. */
  volume: number;
  /** null is ~/Videos/Bloom. */
  downloadLocation: string | null;
  downloadQuality: Quality;
  downloadLocalOnly: boolean;
  /** How many downloads run at once, 1 to 3. */
  downloadParallel: number;
  /** What's playing on the user's Discord profile, through the local Discord app. */
  discordPresence: boolean;
  /** Off, Discord only hears that a show or a film is playing. */
  discordShowTitle: boolean;
  /** A desktop notification for new episodes and films, while Bloom is open. */
  notifyNewMedia: boolean;
};
export type AppInfo = { version: string; glRenderer: string | null; playerReady: boolean; discordReady: boolean };

type ApiError = { code: "signed-out" | "unreachable" | "superseded" | "other"; message: string };

export const settings = () => invoke<Settings>("settings");
/** Resolves to the settings as saved. The zoom level is left alone: it has its own commands. */
export const updateSettings = (settings: Settings) => invoke<Settings>("update_settings", { settings });
export const appInfo = () => invoke<AppInfo>("app_info");

export const startup = () => invoke<Startup>("startup");
export const checkServer = (address: string) => invoke<ServerCheck>("check_server", { address });
/** A Jellyfin server that answered on this network. Only a suggestion: it's checked like a typed address. */
export type DiscoveredServer = { id: string; name: string; address: string };
/** Asks the local network for Jellyfin servers; takes a couple of seconds. */
export const discoverServers = () => invoke<DiscoveredServer[]>("discover_servers");
export const forgetServer = () => invoke<void>("forget_server");
export const signIn = (username: string, password: string, remember: boolean) =>
  invoke<Account>("sign_in", { username, password, remember });
export const quickConnectStart = () => invoke<string>("quick_connect_start");
export const quickConnectPoll = (remember: boolean) => invoke<Account | null>("quick_connect_poll", { remember });
export const quickConnectCancel = () => invoke<void>("quick_connect_cancel");
export const signOut = () => invoke<void>("sign_out");

/** A saved sign-in; the token never leaves Rust. */
export type SavedAccount = { server: Server; user: User; current: boolean };
export type ServerEntry = {
  server: Server;
  /** Seconds since the Unix epoch when the server last answered. */
  lastSeen: number | null;
  current: boolean;
  /** Users with a saved sign-in on it. */
  accounts: User[];
};
export type Reachability = { reachable: boolean; lastSeen: number | null; version: string | null };
export const savedAccounts = () => invoke<SavedAccount[]>("saved_accounts");
export const switchAccount = (serverId: string, userId: string) => invoke<Account>("switch_account", { serverId, userId });
/** Resolves to whether it was the account in use, which leaves Bloom signed out. */
export const forgetAccount = (serverId: string, userId: string) => invoke<boolean>("forget_account", { serverId, userId });
export const servers = () => invoke<ServerEntry[]>("servers");
export const pingServer = (serverId: string) => invoke<Reachability>("ping_server", { serverId });
/** Resolves to whether that signed Bloom out. */
export const removeServer = (serverId: string) => invoke<boolean>("remove_server", { serverId });

/** A poster already in the artwork cache, which loads with nobody signed in. */
export type CachedPoster = { serverId: string; image: Image; width: number };
export const cachedPosters = () => invoke<CachedPoster[]>("cached_posters");
export const home = () => invoke<Section[]>("home");
export const spotlight = () => invoke<Spotlight[]>("spotlight");
export const setFavorite = (itemId: string, favorite: boolean) =>
  invoke<boolean>("set_favorite", { itemId, favorite });

// Playback. The stream itself never reaches the page: Rust hands it straight to mpv.
/** `startSeconds` omitted resumes from the server's resume point. */
export const play = (itemId: string, quality: Quality = "original", startSeconds: number | null = null) =>
  invoke<NowPlaying>("play", { itemId, quality, startSeconds });
/** Resolves once the server has been told where playback stopped. */
export const stopPlayback = () => invoke<void>("playback_stop");
export const togglePause = () => invoke<void>("playback_toggle_pause");
/** The episode after this one in its show, or null. */
export const nextEpisode = (itemId: string) => invoke<UpNext | null>("next_episode", { itemId });
export const onPlayerState = (handler: (state: PlayerState) => void): Promise<UnlistenFn> =>
  listen<PlayerState>("player:state", (e) => handler(e.payload));
export const onPlayerEnded = (handler: (ended: PlayerEnded) => void): Promise<UnlistenFn> =>
  listen<PlayerEnded>("player:ended", (e) => handler(e.payload));
export const onPlayerTracks = (handler: (tracks: Track[]) => void): Promise<UnlistenFn> =>
  listen<{ itemId: string; tracks: Track[] }>("player:tracks", (e) => handler(e.payload.tracks));
export const seek = (seconds: number, relative = false) => invoke<void>("playback_seek", { seconds, relative });
export const setVolume = (volume: number) => invoke<void>("playback_set_volume", { volume });
export const setMuted = (muted: boolean) => invoke<void>("playback_set_muted", { muted });
/** Whether trailers can play in Bloom's player (yt-dlp is installed). */
export const trailersInApp = () => invoke<boolean>("trailers_in_app");
/** Plays an item's trailer (by its number in the item's list) in the player. */
export const playTrailer = (itemId: string, index: number) => invoke<NowPlaying>("play_trailer", { itemId, index });
/** Opens an item's trailer in the browser. */
export const openTrailer = (itemId: string, index: number) => invoke<void>("open_trailer", { itemId, index });
/** 0.25 to 4; 1 is normal. Back to normal when playback stops. */
export const setSpeed = (speed: number) => invoke<void>("playback_set_speed", { speed });
/** Resolves to true when playback has to restart to carry the choice (audio during a transcode). */
export const setTrack = (kind: "audio" | "subtitle", id: number | null) =>
  invoke<boolean>("playback_set_track", { kind, id });
/** Fractions of the window left free on each side of the player box; negative where the box has
 * scrolled past the window's edge. `aspect` is the window's width over its height. */
export const setViewport = (left: number, top: number, right: number, bottom: number, aspect: number) =>
  invoke<void>("player_set_viewport", { left, top, right, bottom, aspect });
export const setFullscreen = (fullscreen: boolean) => invoke<void>("player_set_fullscreen", { fullscreen });

// Live updates from the server (src-tauri/src/live.rs).
/** An item's user data as it now stands on the server. */
export type UserDataChange = {
  itemId: string;
  played: boolean;
  /** 0..1 when partly watched. */
  progress: number | null;
  favorite: boolean;
  unplayedCount: number | null;
};
export type LibraryChange = { added: number; removed: number };
export const onUserDataChanged = (handler: (changes: UserDataChange[]) => void): Promise<UnlistenFn> =>
  listen<UserDataChange[]>("sync:userdata", (e) => handler(e.payload));
export const onLibraryChanged = (handler: (change: LibraryChange) => void): Promise<UnlistenFn> =>
  listen<LibraryChange>("sync:library", (e) => handler(e.payload));
/** The live connection opened, or opened again after a drop during which changes went unheard. */
export const onLiveConnected = (handler: () => void): Promise<UnlistenFn> => listen("sync:connected", () => handler());

// Downloads (src-tauri/src/downloads.rs). The list is the signed-in account's.
export type DownloadStatus = "queued" | "downloading" | "paused" | "failed" | "done";
export type Download = {
  id: string;
  itemId: string;
  kind: string;
  /** The show, for an episode. */
  title: string;
  /** "S1 E4, Name" for an episode, the year for a film. */
  subtitle: string | null;
  seriesId: string | null;
  seasonId: string | null;
  seasonNumber: number | null;
  episodeNumber: number | null;
  image: Image | null;
  quality: Quality;
  /** Original, and the conversions lighter than the file. */
  qualities: Quality[];
  status: DownloadStatus;
  error: string | null;
  /** Why a queued download isn't moving. */
  waiting: "network" | "server" | null;
  bytesDone: number;
  bytesTotal: number | null;
  /** The total is a guess: the server is converting the file as it sends it. */
  estimated: boolean;
  bytesPerSecond: number | null;
  runtimeSeconds: number | null;
  /** Where watching it stopped on this computer. */
  positionSeconds: number;
  played: boolean;
  addedAt: number;
  finishedAt: number | null;
};
export type DownloadStorage = { location: string; bloomBytes: number; freeBytes: number | null; totalBytes: number | null };
export const downloads = () => invoke<Download[]>("downloads");
/** `quality` omitted uses the Settings default. */
export const downloadItem = (itemId: string, quality?: Quality) => invoke<void>("download_item", { itemId, quality });
/** Resolves to how many episodes are queued or already downloaded. */
export const downloadSeason = (seriesId: string, seasonId: string, quality?: Quality) =>
  invoke<number>("download_season", { seriesId, seasonId, quality });
export const pauseDownload = (id: string) => invoke<void>("download_pause", { id });
export const resumeDownload = (id: string) => invoke<void>("download_resume", { id });
/** Cancels a download, or deletes a finished one, with its files. */
export const removeDownload = (id: string) => invoke<void>("download_remove", { id });
export const setDownloadQuality = (id: string, quality: Quality) => invoke<void>("download_set_quality", { id, quality });
export const downloadStorage = () => invoke<DownloadStorage>("download_storage");
/** Creates the folder if needed; resolves to its full path. Empty is the default folder. */
export const checkDownloadLocation = (path: string) => invoke<string>("check_download_location", { path });
/** A server's downloads, for every account on it. */
export type ServerDownloads = { count: number; bytes: number };
export const serverDownloads = (serverId: string) => invoke<ServerDownloads>("server_downloads", { serverId });
/** Deletes every download from a server, with its files. Resolves to how many. */
export const removeServerDownloads = (serverId: string) => invoke<number>("remove_server_downloads", { serverId });
/** Sends resume points recorded offline. */
export const syncDownloads = () => invoke<number>("sync_downloads");
export const onDownloadsChanged = (handler: (list: Download[]) => void): Promise<UnlistenFn> =>
  listen<Download[]>("downloads:changed", (e) => handler(e.payload));

// Library pages. The page size mirrors src-tauri/src/library.rs.
export const GRID_PAGE = 60;

export type LibrarySort = "added" | "name" | "year" | "rating";
export type Grid = { total: number; shape: Shape; cards: Card[] };
export type Season = { id: string; name: string; number: number | null; episodeCount: number; unplayed: number };
export type PlayTarget = {
  itemId: string;
  label: string;
  resume: boolean;
  seasonId: string | null;
  episodeNumber: number | null;
};
export type Person = { id: string; name: string; role: string | null; image: Image | null };
export type MediaInfo = {
  container: string | null;
  sizeBytes: number | null;
  bitrate: number | null;
  video: string[];
  audio: string[];
  subtitles: string[];
};
export type ItemDetail = {
  id: string;
  kind: string;
  title: string;
  year: number | null;
  endYear: number | null;
  status: string | null;
  officialRating: string | null;
  communityRating: number | null;
  genres: string[];
  studios: string[];
  tagline: string | null;
  overview: string | null;
  backdrop: Image | null;
  poster: Image | null;
  /** The title as artwork. */
  logo: Image | null;
  /** Language codes ("jpn"): a film's streams, or a show's first episodes'. */
  audioLanguages: string[];
  subtitleLanguages: string[];
  favorite: boolean;
  played: boolean;
  runtimeMinutes: number | null;
  episodeCount: number | null;
  unplayedCount: number | null;
  seasons: Season[];
  play: PlayTarget | null;
  people: Person[];
  media: MediaInfo | null;
  chapters: Chapter[];
  /** For an episode: its show. */
  series: { id: string; title: string; poster: Image | null } | null;
  seasonId: string | null;
  seasonName: string | null;
  /** Web trailers. Missing from pages saved with downloads before trailers were read. */
  trailers?: { name: string | null; url: string }[];
};
export type Chapter = { name: string | null; startSeconds: number };
export type QueueEntry = {
  id: string;
  title: string;
  meta: string | null;
  image: Image | null;
  runtimeMinutes: number | null;
  progress: number | null;
  played: boolean;
  overview: string | null;
  /** "2026-04-03". */
  premiereDate: string | null;
};
export type Queue = { heading: string; entries: QueueEntry[] };
export type SeasonList = {
  seasons: Season[];
  seasonId: string;
  entries: QueueEntry[];
  /** The first episodes of the season after this one. */
  next: { seasonId: string; name: string; entries: QueueEntry[] } | null;
};
export type Episode = {
  id: string;
  number: number | null;
  title: string;
  overview: string | null;
  runtimeMinutes: number | null;
  image: Image | null;
  played: boolean;
  progress: number | null;
  /** "2026-04-03". */
  premiereDate: string | null;
  /** Language codes of the audio tracks. */
  audioLanguages: string[];
  hasSubtitles: boolean;
};
export type EpisodePage = { total: number; episodes: Episode[] };

export const libraries = () => invoke<Card[]>("libraries");
/** `ids` narrows the grid to those films and shows (the downloaded ones). */
export const libraryItems = (libraryId: string, sort: LibrarySort, start: number, ids?: string[]) =>
  invoke<Grid>("library_items", { libraryId, sort, start, ids });
/** Films and shows with something downloaded, one poster card each. Works with no server. */
export const downloadedTitles = () => invoke<Card[]>("downloaded_titles");
/** `limit` defaults to a grid page. */
export const search = (query: string, limit?: number) => invoke<Card[]>("search", { query, limit });
/** A stretch of a file the player offers to skip. */
export type Segment = { kind: "intro" | "recap" | "credits" | "preview"; startSeconds: number; endSeconds: number };
/** From the server's intro and credits detection, or else guessed from chapter names. */
export const skipSegments = (itemId: string) => invoke<Segment[]>("skip_segments", { itemId });
export const itemDetail = (itemId: string) => invoke<ItemDetail>("item_detail", { itemId });
/** A whole season, in order. */
export const seasonEpisodes = (seriesId: string, seasonId: string) =>
  invoke<EpisodePage>("season_episodes", { seriesId, seasonId });
/** Films and shows like this one. */
export const similar = (itemId: string) => invoke<Card[]>("similar", { itemId });
/** A collection (box set) a title is in, with its titles oldest first. */
export type CollectionRow = { id: string; title: string; cards: Card[] };
export const itemCollections = (itemId: string) => invoke<CollectionRow[]>("item_collections", { itemId });
/** A collection's titles, oldest first. */
export const collectionItems = (collectionId: string) => invoke<Card[]>("collection_items", { collectionId });

/** What a hover card shows for a film, a show or an episode. */
export type CardDetail = {
  id: string;
  kind: string;
  title: string;
  /** For an episode: its show, and "S1 E4". */
  seriesTitle: string | null;
  code: string | null;
  overview: string | null;
  /** "2026-04-03". */
  premiereDate: string | null;
  year: number | null;
  endYear: number | null;
  status: string | null;
  officialRating: string | null;
  communityRating: number | null;
  runtimeMinutes: number | null;
  seasonCount: number | null;
  episodeCount: number | null;
  favorite: boolean;
  played: boolean;
  resume: boolean;
};
export const cardDetail = (itemId: string) => invoke<CardDetail>("card_detail", { itemId });
/** Resolves to whether the item is now marked watched. */
export const setPlayed = (itemId: string, played: boolean) => invoke<boolean>("set_played", { itemId, played });
/** Films like this one. */
export const watchQueue = (itemId: string) => invoke<Queue>("watch_queue", { itemId });
/** One season of a show in full, with the start of the next. */
export const seasonList = (seriesId: string, seasonId: string) =>
  invoke<SeasonList>("season_list", { seriesId, seasonId });

// Tauri spells custom-scheme URLs differently per platform (bloom-img://localhost/ on Linux,
// http://bloom-img.localhost/ on Windows), so let it build the prefix.
const imageBase = (() => {
  try {
    return convertFileSrc("", "bloom-img");
  } catch {
    return ""; // outside Tauri
  }
})();

// Mirrors WIDTHS in src-tauri/src/images.rs. Rounding here too means a zoom step only changes
// an image's URL (and reloads it) when it actually needs a bigger file.
const WIDTHS = [160, 240, 360, 480, 720, 960, 1280, 1920];

/** `cssWidth` is the rendered width in CSS pixels. */
export function imageUrl(image: Image, cssWidth: number): string {
  // WebKit already folds page zoom into devicePixelRatio. Reading the zoom level anyway makes
  // callers recompute after a zoom step, when devicePixelRatio itself has changed.
  void view.zoom;
  const px = cssWidth * (window.devicePixelRatio || 1);
  const width = WIDTHS.find((w) => w >= px) ?? WIDTHS[WIDTHS.length - 1];
  return `${imageBase}${image.itemId}/${image.kind}/${image.tag}/${width}`;
}

/** A cached poster, named by its server and at exactly its cached width: read from disk only. */
export function cachedImageUrl(poster: CachedPoster): string {
  const { itemId, kind, tag } = poster.image;
  return `${imageBase}${poster.serverId}/${itemId}/${kind}/${tag}/${poster.width}`;
}

/** Seek-bar thumbnails: tiles of tileWidth × tileHeight thumbnails, one every intervalMs. */
export type Trickplay = {
  itemId: string;
  mediaSourceId: string;
  width: number;
  height: number;
  tileWidth: number;
  tileHeight: number;
  count: number;
  intervalMs: number;
};
/** null where the server hasn't made thumbnails for the item. */
export const trickplay = (itemId: string) => invoke<Trickplay | null>("trickplay", { itemId });

/** One thumbnail, `index` counting from the start of the file. Rust cuts it out of its tile, so the
 *  page never decodes a whole tile (about 23 MB) to show one frame. */
export function trickplayThumbUrl(t: Trickplay, index: number): string {
  const perTile = t.tileWidth * t.tileHeight;
  const tile = Math.floor(index / perTile);
  const n = index % perTile;
  return `${imageBase}trickplay/${t.itemId}/${t.mediaSourceId}/${t.width}/${t.tileWidth}x${t.tileHeight}/${tile}/${n}`;
}

function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && typeof (e as ApiError).message === "string";
}

export function message(e: unknown): string {
  if (isApiError(e)) return e.message;
  if (typeof e === "string") return e;
  return e instanceof Error ? e.message : "Something went wrong.";
}

export const isSignedOut = (e: unknown) => isApiError(e) && e.code === "signed-out";
/** A play request that gave way to a newer play or stop: nothing to show. */
export const isSuperseded = (e: unknown) => isApiError(e) && e.code === "superseded";
