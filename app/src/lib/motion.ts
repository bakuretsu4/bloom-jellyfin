// Opening a title (DESIGN.md §7): the artwork that was clicked grows into what opens, through a
// View Transition. The card marks itself as the source when clicked; the page that opens marks
// its landing place with `data-morph-target`.
import { tick } from "svelte";
import { reducedMotion } from "./settings.svelte";

type ViewTransitionDocument = Document & {
  startViewTransition?: (update: () => Promise<void>) => { finished: Promise<void> };
};

/** A mark older than this belongs to some earlier click that didn't open anything. */
const SOURCE_TTL_MS = 1000;
let source: { el: HTMLElement; at: number } | null = null;

/** Called by artwork as it's clicked, just before whatever it opens. */
export function markSource(el: Element | null | undefined) {
  source = el instanceof HTMLElement ? { el, at: performance.now() } : null;
}

/** The artwork just clicked, if any, used once. */
export function takeSource(): HTMLElement | null {
  const taken = source && performance.now() - source.at < SOURCE_TTL_MS && source.el.isConnected ? source.el : null;
  source = null;
  return taken;
}

const NAME = "title-art";

/**
 * Runs `update` (the navigation) inside a View Transition that morphs `from` into the element
 * marked `data-morph-target` once the new page is in. Without support, without a source, or with
 * reduced motion, it's a plain change.
 */
export async function morph(from: HTMLElement | null, update: () => void | Promise<void>) {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || !from || reducedMotion()) {
    await update();
    return;
  }
  from.style.setProperty("view-transition-name", NAME);
  const transition = doc.startViewTransition(async () => {
    from.style.removeProperty("view-transition-name");
    await update();
    await tick();
    document.querySelector<HTMLElement>("[data-morph-target]")?.style.setProperty("view-transition-name", NAME);
  });
  await transition.finished.catch(() => {});
  document.querySelector<HTMLElement>("[data-morph-target]")?.style.removeProperty("view-transition-name");
}
