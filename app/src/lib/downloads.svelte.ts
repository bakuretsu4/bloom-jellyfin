// The signed-in account's downloads, kept current from Rust's downloads:changed events, for the
// Downloads screen and every Download button.
import * as api from "./api";

export const downloads = $state<{ list: api.Download[] }>({ list: [] });

let listening: Promise<() => void> | null = null;

/** Reads the list and follows its changes. Called whenever an account comes into the shell. */
export async function trackDownloads() {
  listening ??= api.onDownloadsChanged((list) => (downloads.list = list));
  try {
    downloads.list = await api.downloads();
  } catch {
    downloads.list = [];
  }
}

/** The films and shows with something downloaded, a show by its own id. */
export function downloadedTitleIds(): string[] {
  const done = downloads.list.filter((d) => d.status === "done");
  return [...new Set(done.map((d) => (d.kind === "Episode" && d.seriesId ? d.seriesId : d.itemId)))];
}

/** A film, episode or show (any of its episodes) is downloaded. */
export function hasDownloaded(id: string): boolean {
  return downloads.list.some((d) => d.status === "done" && (d.itemId === id || d.seriesId === id));
}

export function downloadOf(itemId: string): api.Download | undefined {
  return downloads.list.find((d) => d.itemId === itemId);
}

/** 0..1. An estimated total never reads as finished before the file is. */
export function downloadFraction(d: api.Download): number {
  if (d.status === "done") return 1;
  if (!d.bytesTotal) return 0;
  return Math.min(d.bytesDone / d.bytesTotal, d.estimated ? 0.99 : 1);
}

export function bytesLabel(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}
