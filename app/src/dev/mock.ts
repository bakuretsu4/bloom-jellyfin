// Dev only (`npm run dev`, open http://localhost:1420/?mock): stands in for the Rust side so the
// interface can be developed and screenshotted in a plain browser. Artwork comes from the
// mock-img middleware in vite.config.ts. Not part of the production build.
import type { Card, ItemDetail, Section, Settings, Spotlight } from "../lib/api";

const TITLES = [
  ["Northern Lights", "Movie", 2023], ["The Long Tide", "Series", 2021], ["Paper Moons", "Movie", 2019],
  ["Signal Fire", "Series", 2024], ["Glass Harbor", "Movie", 2022], ["Second Winter", "Series", 2020],
  ["Ember & Ash", "Movie", 2018], ["Quiet Orbit", "Movie", 2025], ["The Cartographer", "Series", 2017],
  ["Low Season", "Movie", 2016], ["Velvet Static", "Series", 2023], ["Kestrel", "Movie", 2021],
  ["Salt Road", "Series", 2019], ["Afterglow", "Movie", 2024], ["Nine Doors", "Series", 2022],
  ["Harvest Moon", "Movie", 2015], ["Undertow", "Movie", 2020], ["Winter Garden", "Series", 2018],
] as const;

const img = (id: string, kind: "Primary" | "Thumb" | "Backdrop" | "Logo" = "Primary") => ({ itemId: id, kind, tag: "t" });
const idOf = (i: number) => `item-${i}`;
const card = (i: number, extra: Partial<Card> = {}): Card => ({
  id: idOf(i), kind: TITLES[i][1], title: TITLES[i][0], meta: `${TITLES[i][2]}`, image: img(idOf(i)),
  progress: null, ...extra,
});
const cards = (from: number, n: number, extra: (i: number) => Partial<Card> = () => ({})) =>
  Array.from({ length: n }, (_, k) => card((from + k) % TITLES.length, extra(k)));

const settings: Settings = {
  zoom: 1, theme: "auto", accent: "amber", customThemes: [], customTheme: null, reduceMotion: false,
  hwdec: "auto", directPlay: true, maxQuality: "original", audioLanguage: null, subtitleMode: "server",
  subtitleLanguage: null, subtitleSize: "normal", subtitleBackground: "outline", autoplayNext: true,
  volume: 80, downloadLocation: null, downloadQuality: "original", downloadLocalOnly: false,
  downloadParallel: 2, discordPresence: false, discordShowTitle: false, notifyNewMedia: false,
};
const server = { address: "http://jellyfin.local:8096", id: "srv1", name: "Home Server", version: "10.10.0" };
const user = { id: "u1", name: "alyx" };

const home = (): Section[] => [
  { id: "resume", title: "Continue Watching", shape: "wide", cards: cards(0, 6, (k) => ({ progress: 0.15 + k * 0.13, image: img(idOf(k), "Thumb"), meta: "S1 · E" + (k + 2) })) },
  { id: "next", title: "Next Up", shape: "wide", cards: cards(6, 5, (k) => ({ image: img(idOf(k + 6), "Thumb"), kind: "Episode", meta: "S2 · E" + (k + 1) })) },
  { id: "added", title: "Recently Added Movies", shape: "poster", cards: cards(2, 12) },
  { id: "shows", title: "Recently Added Shows", shape: "poster", cards: cards(1, 12) },
];
const spot = (i: number): Spotlight => ({
  id: idOf(i), kind: TITLES[i][1], title: TITLES[i][0], backdrop: img(idOf(i), "Backdrop"), logo: null,
  overview: "A quiet, patient story about the people who keep the lights on when everyone else has left, and what they find when the signal finally comes back.",
  year: TITLES[i][2], officialRating: "PG-13", communityRating: 7.8, genres: ["Drama", "Sci-Fi"],
  runtimeMinutes: 118, seasons: TITLES[i][1] === "Series" ? 3 : null, favorite: false,
});
const detail = (id: string): ItemDetail => {
  const i = Math.max(0, Number(id.replace("item-", "")) || 0) % TITLES.length;
  const series = TITLES[i][1] === "Series";
  return {
    id, kind: TITLES[i][1], title: TITLES[i][0], year: TITLES[i][2], endYear: null, status: null, officialRating: "PG-13",
    communityRating: 7.8, genres: ["Drama", "Sci-Fi", "Mystery"], studios: ["Northlight"], tagline: "Some signals are worth waiting for.",
    overview: "A quiet, patient story about the people who keep the lights on when everyone else has left, and what they find when the signal finally comes back.",
    backdrop: img(id, "Backdrop"), poster: img(id), logo: null, audioLanguages: ["eng", "jpn"], subtitleLanguages: ["eng", "spa"],
    favorite: false, played: false, runtimeMinutes: 118, episodeCount: series ? 24 : null, unplayedCount: series ? 20 : null,
    seasons: series ? [1, 2, 3].map((n) => ({ id: `s${n}`, name: `Season ${n}`, number: n, episodeCount: 8, unplayed: 6 })) : [],
    play: { itemId: id, label: series ? "Resume S1 · E3" : "Play", resume: series, seasonId: "s1", episodeNumber: 3 },
    people: Array.from({ length: 10 }, (_, k) => ({ id: `p${k}`, name: ["Mara Quill", "Jonas Reed", "Ivy Tan", "Omar Vega", "Lena Ford"][k % 5], role: "Character " + (k + 1), image: null })),
    media: { container: "mkv", sizeBytes: 8.2e9, bitrate: 24_000_000, video: ["HEVC 3840x2160 HDR10"], audio: ["English TrueHD 7.1", "Japanese AAC 2.0"], subtitles: ["English", "Spanish"] },
    chapters: [], series: null, seasonId: null, seasonName: null,
  };
};

const handlers: Record<string, (args: any) => unknown> = {
  startup: () => ({ account: { server, user }, offline: false, lastAddress: server.address }),
  settings: () => settings,
  update_settings: (a) => Object.assign(settings, a.settings),
  app_info: () => ({ version: "0.1.0", glRenderer: "mock", playerReady: true, discordReady: false }),
  home, spotlight: () => [0, 7, 3].map(spot),
  libraries: () => [{ id: "lib-movies", kind: "CollectionFolder", title: "Movies", meta: "312 titles", image: img("lib-movies", "Thumb"), progress: null }, { id: "lib-shows", kind: "CollectionFolder", title: "Shows", meta: "88 titles", image: img("lib-shows", "Thumb"), progress: null }],
  library_items: () => ({ total: TITLES.length, shape: "poster", cards: cards(0, TITLES.length) }),
  item_detail: (a) =>
    String(a.itemId).startsWith("ep")
      ? { ...detail("item-1"), id: a.itemId, kind: "Episode", title: "The Signal", seasonId: "s1", seasonName: "Season 1", series: { id: "item-1", title: "The Long Tide", poster: img("item-1") }, seasons: [], play: null }
      : detail(a.itemId),
  season_list: (a) => ({ seasons: detail("item-1").seasons, seasonId: a.seasonId ?? "s1", next: null, entries: Array.from({ length: 8 }, (_, k) => ({ id: `ep${k}`, title: `Episode ${k + 1}`, meta: `S1 · E${k + 1}`, image: img(`ep${k}`, "Thumb"), runtimeMinutes: 42, progress: k === 2 ? 0.4 : null, played: k < 2, overview: "The crew follows a faint signal past the edge of the map.", premiereDate: "2023-04-0" + (k + 1) })) }),
  season_episodes: () => ({ total: 8, episodes: Array.from({ length: 8 }, (_, k) => ({ id: `ep${k}`, number: k + 1, title: ["Pilot", "The Signal", "Low Tide", "Static", "Northbound", "Ashes", "The Long Way", "Finale"][k], overview: "The crew follows a faint signal past the edge of the map.", runtimeMinutes: 42, image: img(`ep${k}`, "Thumb"), played: k < 2, progress: k === 2 ? 0.4 : null, premiereDate: "2023-04-0" + (k + 1), audioLanguages: ["eng", "jpn"], hasSubtitles: true })) }),
  search: () => cards(0, 8), similar: () => cards(4, 8), item_collections: () => [], downloads: () => [],
  saved_accounts: () => [{ server, user, current: true }], servers: () => [{ server, lastSeen: 1, current: true, accounts: [user] }],
  cached_posters: () => [], downloaded_titles: () => [], download_storage: () => ({ used: 0, free: 1e11 }),
};

// Events: the page listens through plugin:event, so the mock keeps the callbacks and can emit.
const callbacks = new Map<number, (msg: unknown) => void>();
const listeners: { event: string; id: number }[] = [];
let nextId = 1;
const emit = (event: string, payload: unknown) =>
  listeners.filter((l) => l.event === event).forEach((l) => callbacks.get(l.id)?.({ event, id: l.id, payload }));

// A pretend player: position advances once a second so the controls move.
let ticker: ReturnType<typeof setInterval> | undefined;
const playback = { itemId: "", position: 600, paused: false, volume: 80, muted: false, speed: 1 };
function startPlayback(itemId: string) {
  Object.assign(playback, { itemId, position: 600, paused: false });
  clearInterval(ticker);
  const send = () =>
    emit("player:state", { ...playback, duration: 2520, buffering: false, started: true, decoder: "mock", cacheEnd: playback.position + 180, audioTrack: 1, subtitleTrack: null });
  ticker = setInterval(() => { if (!playback.paused) playback.position += 1; send(); }, 1000);
  setTimeout(() => {
    emit("player:tracks", { itemId, tracks: [
      { id: 1, kind: "audio", ffIndex: 1, title: "English", lang: "eng", codec: "truehd", channels: 8, width: null, height: null, fps: null, default: true, forced: false, external: false, hearingImpaired: false, selected: true },
      { id: 2, kind: "sub", ffIndex: 2, title: "English", lang: "eng", codec: "subrip", channels: null, width: null, height: null, fps: null, default: false, forced: false, external: false, hearingImpaired: false, selected: false },
    ] });
    send();
  }, 300);
}

Object.assign(handlers, {
  play: (a: any) => { startPlayback(a.itemId); return { itemId: a.itemId, kind: "Episode", title: "The Signal", subtitle: "Signal Fire · S1 · E2", overview: null, method: "DirectPlay", transcoding: false, durationSeconds: 2520, startSeconds: 600, quality: "original", source: { width: 3840, height: 2160, codec: "hevc", bitrate: 24e6 }, downloaded: false }; },
  playback_toggle_pause: () => { playback.paused = !playback.paused; },
  playback_seek: (a: any) => { playback.position = a.relative ? playback.position + a.seconds : a.seconds; },
  playback_set_volume: (a: any) => { playback.volume = a.volume; },
  playback_set_muted: (a: any) => { playback.muted = a.muted; },
  playback_stop: () => clearInterval(ticker),
  watch_queue: () => ({ heading: "More like this", entries: cards(2, 6).map((c) => ({ id: c.id, title: c.title, meta: c.meta, image: c.image, runtimeMinutes: 110, progress: null, played: false, overview: null, premiereDate: null })) }),
  skip_segments: () => [], trickplay: () => null, next_episode: () => ({ itemId: "ep3", title: "Low Tide", subtitle: "S1 · E3", image: img("ep3", "Thumb") }),
});

export function installMock() {
  (window as any).__TAURI_INTERNALS__ = {
    convertFileSrc: (path: string, protocol = "asset") => (protocol === "bloom-img" ? "/mock-img/" : `/${protocol}/`) + path,
    transformCallback: (cb: (msg: unknown) => void) => { const id = nextId++; callbacks.set(id, cb); return id; },
    unregisterCallback: (id: number) => callbacks.delete(id),
    invoke: async (cmd: string, args: any) => {
      if (cmd === "plugin:event|listen") { listeners.push({ event: args.event, id: args.handler }); return args.handler; }
      if (cmd.startsWith("plugin:event")) return 0;
      const h = handlers[cmd];
      return h ? h(args) : null;
    },
  };
}
