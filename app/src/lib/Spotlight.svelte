<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import * as api from "./api";
  import Icon from "./Icon.svelte";
  import { forgetLive } from "./live.svelte";
  import { markSource } from "./motion";
  import { reducedMotion } from "./settings.svelte";

  let {
    items,
    onSignedOut,
    onPlay,
    onOpen,
  }: {
    items: api.Spotlight[];
    onSignedOut: () => void;
    onPlay: (itemId: string) => void;
    /** The slide itself was chosen: open the title's page. */
    onOpen: (card: api.Card) => void;
  } = $props();

  let section = $state<HTMLElement>();

  /** The title's page, grown from the backdrop on screen. */
  function open() {
    markSource(section?.querySelector<HTMLElement>(".backdrop.is-current"));
    onOpen({ id: current.id, kind: current.kind, title: current.title, meta: null, image: null, progress: null });
  }

  /** A click anywhere on the slide opens it, except on its own controls. */
  function clickSlide(e: MouseEvent) {
    if ((e.target as Element | null)?.closest("button")) return;
    open();
  }

  // How long a slide stays before the next. The pager bar fills in steps over the same time, and
  // its last step is what advances, so the bar and the slide can't drift apart. Steps, not a CSS
  // animation: the 30px bar gains a pixel about every 300ms, while an animation had the page draw
  // a frame at the display's rate for as long as Home was open, idle or not.
  const DWELL_MS = 9000;
  const STEPS = 30;
  let autoAdvance = $derived(!reducedMotion() && items.length > 1);

  let index = $state(0);
  let step = $state(0);
  let hovered = $state(false);
  let focused = $state(false);
  let pageHidden = $state(document.hidden);
  /** Scrolled away, or Home hidden under another page, and it waits. */
  let inView = $state(true);
  let now = $state(Date.now());
  let favorites = $state<Record<string, boolean>>({});
  let favoriteBusy = $state(false);
  let brokenLogos = new SvelteSet<string>();

  let current = $derived(items[index]);
  let isFavorite = $derived(favorites[current.id] ?? current.favorite);
  let running = $derived(!hovered && !focused && !pageHidden && inView);
  // Rendered widths at 100% zoom, for asking Rust for the right image size.
  const backdropWidth = Math.min(window.innerWidth, 1920);

  /** Backdrops kept loaded: the slide on screen, the one fading out, and the one coming next. */
  function near(i: number): boolean {
    const n = items.length;
    return n <= 3 || i === index || i === (index + 1) % n || i === (index - 1 + n) % n;
  }

  function go(i: number) {
    index = (i + items.length) % items.length;
    step = 0;
  }

  $effect(() => {
    const onVisibility = () => (pageHidden = document.hidden);
    document.addEventListener("visibilitychange", onVisibility);
    const observer = new IntersectionObserver(([entry]) => (inView = entry.isIntersecting));
    if (section) observer.observe(section);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      observer.disconnect();
    };
  });

  // The pager's steps, only while the slide is on screen and nothing holds it.
  $effect(() => {
    if (!autoAdvance || !running) return;
    const timer = setInterval(() => {
      if (step + 1 >= STEPS) go(index + 1);
      else step += 1;
    }, DWELL_MS / STEPS);
    return () => clearInterval(timer);
  });

  // "Ends at" follows the clock, while anyone can see it.
  $effect(() => {
    if (!running) return;
    now = Date.now();
    const clock = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(clock);
  });

  async function toggleFavorite() {
    const item = current;
    favoriteBusy = true;
    try {
      favorites[item.id] = await api.setFavorite(item.id, !(favorites[item.id] ?? item.favorite));
      // A change made here wins over an older one heard from the server.
      forgetLive(item.id);
    } catch (e) {
      if (api.isSignedOut(e)) onSignedOut();
    } finally {
      favoriteBusy = false;
    }
  }

  function runtime(minutes: number): string {
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return h ? (m ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
  }

  const clock = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
  const endsAt = (minutes: number) => clock.format(new Date(now + minutes * 60_000));
</script>

<!-- The whole slide is a pointer target; the title button is the same action for the keyboard. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<section
  class="spot"
  bind:this={section}
  aria-roledescription="carousel"
  aria-label="Spotlight"
  onclick={clickSlide}
  onpointerenter={() => (hovered = true)}
  onpointerleave={() => (hovered = false)}
  onfocusin={() => (focused = true)}
  onfocusout={(e) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node | null)) focused = false;
  }}
>
  <div class="backdrops" aria-hidden="true">
    {#each items as item, i (item.id)}
      {#if near(i)}
        <img
          class="backdrop"
          class:is-current={i === index}
          src={api.imageUrl(item.backdrop, backdropWidth)}
          alt=""
          decoding="async"
        />
      {/if}
    {/each}
    <div class="veil"></div>
  </div>

  {#key current.id}
    <div class="content" aria-roledescription="slide" aria-label={`${index + 1} of ${items.length}`}>
      <h2 class="heading">
        <button class="open" aria-label="{current.title}, open details" onclick={open}>
          {#if current.logo && !brokenLogos.has(current.id)}
            <img
              class="logo"
              src={api.imageUrl(current.logo, 460)}
              alt={current.title}
              onerror={() => brokenLogos.add(current.id)}
            />
          {:else}
            <span class="title">{current.title}</span>
          {/if}
        </button>
      </h2>

      <div class="meta-line">
        {#if current.communityRating}<span>{current.communityRating.toFixed(1)}/10</span><span class="sep"></span>{/if}
        {#if current.year}<span>{current.year}</span>{/if}
        {#if current.officialRating}<span class="tag">{current.officialRating}</span>{/if}
        {#if current.runtimeMinutes}
          <span class="sep"></span><span>{runtime(current.runtimeMinutes)}</span>
          <span class="sep"></span><span>Ends at {endsAt(current.runtimeMinutes)}</span>
        {:else if current.seasons}
          <span class="sep"></span><span>{current.seasons} {current.seasons === 1 ? "season" : "seasons"}</span>
        {/if}
      </div>

      {#if current.genres.length}<p class="genres">{current.genres.join(", ")}</p>{/if}
      {#if current.overview}<p class="overview">{current.overview}</p>{/if}

      <div class="actions">
        <button class="btn btn-primary" onclick={() => onPlay(current.id)}>
          <Icon name="play" size={16} />Play
        </button>
        <button class="btn" aria-pressed={isFavorite} disabled={favoriteBusy} onclick={toggleFavorite}>
          <Icon name="heart" size={16} />{isFavorite ? "Favorited" : "Favorite"}
        </button>
      </div>
    </div>
  {/key}

  {#if items.length > 1}
    <div class="pager">
      {#each items as item, i (item.id)}
        <button
          class="seg"
          aria-label={`Show ${i + 1} of ${items.length}: ${item.title}`}
          aria-current={i === index ? "true" : undefined}
          onclick={() => go(i)}
        >
          {#if i === index}
            <span class="fill" class:is-timed={autoAdvance} style:--progress={step / STEPS}></span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</section>

<style>
  .spot {
    position: relative;
    height: clamp(380px, 58vh, 640px);
    overflow: hidden;
    display: flex;
    align-items: flex-end;
    background: var(--ground);
  }

  .backdrops {
    position: absolute;
    inset: 0;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: 70% 22%;
    opacity: 0;
    transition: opacity 0.7s var(--ease);
  }
  .backdrop.is-current {
    opacity: 1;
  }
  /* Legibility for the text, and a seam-free hand-off into the rails below. */
  .veil {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(
        90deg,
        var(--ground) 0%,
        color-mix(in oklab, var(--ground) 86%, transparent) 28%,
        color-mix(in oklab, var(--ground) 30%, transparent) 58%,
        transparent 78%
      ),
      linear-gradient(0deg, var(--ground) 0%, color-mix(in oklab, var(--ground) 60%, transparent) 18%, transparent 42%);
  }

  .content {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    max-width: min(640px, 100%);
    padding-inline: clamp(16px, 2.2vw, 26px);
    padding-block: 0 clamp(26px, 5vh, 48px);
    animation: rise 0.42s var(--ease-mech) both;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .spot {
    cursor: pointer;
  }
  .heading {
    margin: 0 0 6px;
  }
  .open {
    display: block;
    padding: 0;
    border: 0;
    border-radius: var(--r);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .open:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 4px;
  }
  .logo {
    display: block;
    max-width: min(420px, 80%);
    max-height: clamp(64px, 12vh, 130px);
    object-fit: contain;
    object-position: left bottom;
  }
  .title {
    display: block;
    font-stretch: 118%;
    font-weight: 600;
    letter-spacing: -0.02em;
    font-size: clamp(1.9rem, 3.6vw, 3rem);
    line-height: 1.02;
    overflow-wrap: anywhere;
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
    background: var(--line);
    flex: none;
  }
  .tag {
    font-size: 11.5px;
    color: var(--ink-2);
    border: 1px solid var(--line);
    border-radius: 5px;
    padding: 1px 6px;
    white-space: nowrap;
  }
  .genres {
    margin: 0;
    font-size: 13.5px;
    color: var(--ink-2);
  }
  .overview {
    margin: 0;
    white-space: pre-line;
    max-width: 62ch;
    font-size: 14px;
    line-height: 1.6;
    color: var(--ink);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .actions {
    display: flex;
    gap: 10px;
    margin-top: 8px;
  }
  .actions .btn {
    height: 40px;
    padding: 0 16px;
    font-size: 14px;
  }
  .actions .btn[aria-pressed="true"] {
    color: var(--accent);
    border-color: color-mix(in oklab, var(--accent) 42%, var(--line));
  }

  .pager {
    position: absolute;
    z-index: 1;
    right: clamp(16px, 2.2vw, 26px);
    bottom: clamp(26px, 5vh, 48px);
    display: flex;
    gap: 6px;
  }
  .seg {
    position: relative;
    width: 30px;
    height: 18px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }
  .seg::before,
  .fill {
    position: absolute;
    left: 0;
    top: 8px;
    height: 3px;
    border-radius: 1px;
  }
  .seg::before {
    content: "";
    right: 0;
    background: color-mix(in oklab, var(--ink) 24%, transparent);
    transition: background 0.16s var(--ease);
  }
  .seg:hover::before {
    background: color-mix(in oklab, var(--ink) 45%, transparent);
  }
  .fill {
    right: 0;
    background: var(--accent);
    transform-origin: left center;
  }
  /* A transform, not width, so a step doesn't re-run layout. */
  .fill.is-timed {
    transform: scaleX(var(--progress, 0));
  }

  @media (max-width: 700px) {
    .pager {
      top: 14px;
      bottom: auto;
    }
  }
</style>
