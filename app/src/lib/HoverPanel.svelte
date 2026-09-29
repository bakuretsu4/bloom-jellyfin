<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  let {
    anchor,
    label,
    fit = "grow",
    width = 300,
    onEnter,
    onLeave,
    onClose,
    onActivate,
    children,
  }: {
    /** What the card lies over. */
    anchor: HTMLElement;
    label: string;
    /** "cover" lays the card exactly over the anchor (a poster's artwork, an episode tile), never
     * past it; "grow" makes it at least `width` wide and as tall as it needs, for rows too short
     * to hold it. */
    fit?: "cover" | "grow";
    width?: number;
    onEnter: (e: PointerEvent) => void;
    onLeave: () => void;
    onClose: () => void;
    /** A click on the card outside its own buttons: whatever clicking the tile does. */
    onActivate?: () => void;
    children: Snippet;
  } = $props();

  const MARGIN = 8;

  // Measured once, when the card opens; it closes on any scroll or resize rather than follow.
  // Fixed, so a rail's or list's overflow can't clip it.
  const place = untrack(() => {
    const rect = anchor.getBoundingClientRect();
    if (fit === "cover") return { rect, cover: true, left: rect.left, cardWidth: rect.width };
    const cardWidth = Math.min(Math.max(rect.width, width), window.innerWidth - 2 * MARGIN);
    const left = Math.min(Math.max(MARGIN, rect.left + rect.width / 2 - cardWidth / 2), window.innerWidth - cardWidth - MARGIN);
    return { rect, cover: false, left, cardWidth };
  });
  let top = $state(place.rect.top);
  let panel = $state<HTMLDivElement>();

  // A growing card sits over the tile, moved up only as far as it takes to stay inside the
  // window. Its height can change once its detail loads.
  $effect(() => {
    const el = panel;
    if (!el || place.cover) return;
    const fitWindow = () => {
      top = Math.max(MARGIN, Math.min(place.rect.top, window.innerHeight - el.offsetHeight - MARGIN));
    };
    fitWindow();
    const observer = new ResizeObserver(fitWindow);
    observer.observe(el);
    return () => observer.disconnect();
  });

  $effect(() => {
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("scroll", close, { capture: true, passive: true });
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("scroll", close, { capture: true });
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKey);
    };
  });

  /** The nearest thing the anchor scrolls inside, along one axis. */
  function scrollerFor(el: HTMLElement, axis: "x" | "y"): Element | null {
    for (let node = el.parentElement; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      const overflow = axis === "y" ? style.overflowY : style.overflowX;
      const room = axis === "y" ? node.scrollHeight > node.clientHeight : node.scrollWidth > node.clientWidth;
      if (room && (overflow === "auto" || overflow === "scroll")) return node;
    }
    return document.scrollingElement;
  }

  // Fixed, the card sits outside every scrolling box, so the wheel over it would scroll nothing.
  // The first wheel event is handed to what the tile scrolls inside, smoothly, and the card gets
  // out of the way at once: it stops taking the pointer and closes, so the rest of the gesture
  // reaches the page itself, with the page's own smooth and kinetic scrolling. Not passive, so
  // the card's own default can be dropped.
  $effect(() => {
    const el = panel;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      const scale = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? window.innerHeight : 1;
      const dx = e.deltaX * scale;
      const dy = (e.shiftKey && !e.deltaX ? 0 : e.deltaY) * scale;
      const sideways = e.shiftKey && !e.deltaX ? e.deltaY * scale : dx;
      const target = scrollerFor(anchor, Math.abs(dy) >= Math.abs(sideways) ? "y" : "x");
      el.style.pointerEvents = "none";
      if (target) {
        e.preventDefault();
        target.scrollBy({ left: sideways, top: dy, behavior: "smooth" });
      }
      onClose();
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  function activate(e: MouseEvent) {
    if ((e.target as Element | null)?.closest("button, a, [role='menu']")) return;
    onActivate?.();
  }
</script>

<!-- A pointer affordance over a tile that stays keyboard-reachable on its own. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions, a11y_no_static_element_interactions -->
<div
  class="hover-card"
  class:is-cover={place.cover}
  class:is-link={!!onActivate}
  role="group"
  aria-label={label}
  bind:this={panel}
  style:left="{place.left}px"
  style:top="{top}px"
  style:width="{place.cardWidth}px"
  style:height={place.cover ? `${place.rect.height}px` : undefined}
  style:min-height={place.cover ? undefined : `${place.rect.height}px`}
  onpointerenter={onEnter}
  onpointerleave={onLeave}
  onclick={activate}
>
  {@render children()}
</div>

<style>
  .hover-card {
    position: fixed;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px 10px;
    background: var(--md-sys-color-surface-container-high);
    border-radius: var(--md-sys-shape-lg);
    box-shadow: var(--md-sys-elevation-3);
    color: var(--md-sys-color-on-surface);
    text-align: left;
    white-space: normal;
    /* The contents adapt to a narrow card (a poster in a rail) with container queries. */
    container-type: inline-size;
    animation: lift var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized-decelerate) both;
  }
  /* Exactly the artwork's size: tighter, and nothing may spill past it. */
  .hover-card.is-cover {
    gap: 5px;
    padding: 10px 11px 6px;
    overflow: hidden;
    border-radius: var(--md-sys-shape-lg);
  }
  .hover-card.is-link {
    cursor: pointer;
  }
  @keyframes lift {
    from {
      opacity: 0;
      transform: scale(0.94);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>
