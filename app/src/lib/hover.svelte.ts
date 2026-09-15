// Hover cards: the detail a tile shows after the pointer rests on it (see DESIGN.md, Hover cards).
import * as api from "./api";

const OPEN_DELAY_MS = 450;
/** Time to move the pointer from a tile onto its card without the card closing. */
const CLOSE_GRACE_MS = 140;

let openIntent: HoverIntent | null = null;

/**
 * Which tile in a group (a grid, a list, one card) has its hover card open. Mouse only: touch has
 * no hover, and a tap should just activate. One card is open across the app at a time.
 */
export class HoverIntent {
  key = $state<string | null>(null);
  #show: ReturnType<typeof setTimeout> | undefined;
  #hide: ReturnType<typeof setTimeout> | undefined;

  enter = (key: string, e: PointerEvent) => {
    if (e.pointerType !== "mouse") return;
    clearTimeout(this.#hide);
    if (this.key === key) return;
    clearTimeout(this.#show);
    this.#show = setTimeout(() => {
      if (openIntent && openIntent !== this) openIntent.close();
      openIntent = this;
      this.key = key;
    }, OPEN_DELAY_MS);
  };

  leave = () => {
    clearTimeout(this.#show);
    clearTimeout(this.#hide);
    this.#hide = setTimeout(this.close, CLOSE_GRACE_MS);
  };

  close = () => {
    clearTimeout(this.#show);
    clearTimeout(this.#hide);
    this.key = null;
    if (openIntent === this) openIntent = null;
  };
}

const FRESH_MS = 60_000;
const details = new Map<string, { at: number; value: Promise<api.CardDetail> }>();

/** A title's hover-card detail, asked for at most once a minute. */
export function cardDetail(id: string): Promise<api.CardDetail> {
  const hit = details.get(id);
  if (hit && Date.now() - hit.at < FRESH_MS) return hit.value;
  const value = api.cardDetail(id);
  details.set(id, { at: Date.now(), value });
  value.catch(() => details.delete(id));
  // Stale details go once there are many, so a long session's browsing doesn't pile up.
  if (details.size > 300) {
    for (const [key, entry] of details) if (Date.now() - entry.at >= FRESH_MS) details.delete(key);
  }
  return value;
}

/** After switching account: favorites and watched marks are another user's. */
export function clearCardDetails() {
  details.clear();
}

/** After a change (a favorite, a watched mark), so the next card reads it fresh. */
export function forgetCardDetail(id: string) {
  details.delete(id);
}

const dates = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" });

/** "3 Apr 2026", in the viewer's own format, from "2026-04-03". */
export function airDate(iso: string | null): string | null {
  if (!iso) return null;
  const date = new Date(`${iso}T00:00:00`);
  return Number.isNaN(date.getTime()) ? null : dates.format(date);
}

export function runtime(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h ? (m ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
}
