<script lang="ts">
  import * as api from "./api";
  import Art from "./Art.svelte";
  import HoverPanel from "./HoverPanel.svelte";
  import Icon from "./Icon.svelte";
  import { hasDownloaded } from "./downloads.svelte";
  import { live } from "./live.svelte";
  import TitleHover from "./TitleHover.svelte";
  import { HoverIntent } from "./hover.svelte";
  import { markSource } from "./motion";

  let {
    card,
    shape,
    width,
    fluid = false,
    onActivate,
    onPlay,
  }: {
    card: api.Card;
    shape: api.Shape;
    /** Rendered width at 100% zoom: the card's width in a rail, the artwork size to ask for in a grid. */
    width: number;
    /** Fill a grid cell instead of keeping a fixed width. */
    fluid?: boolean;
    onActivate?: (card: api.Card) => void;
    /** Given, a film, show or episode gets a hover card with Play. */
    onPlay?: (itemId: string) => void;
  } = $props();

  const HOVERS = new Set(["Movie", "Series", "Episode", "Video", "MusicVideo"]);
  const hover = new HoverIntent();
  /** The artwork, which the hover card lies over. */
  let shot = $state<HTMLElement>();
  let hovers = $derived(!!onPlay && HOVERS.has(card.kind));
  /** Something of it is downloaded: the film or episode, or any episode of the show. */
  let saved = $derived(HOVERS.has(card.kind) && hasDownloaded(card.id));
  /** The server's latest, when it has said something changed since the card loaded. */
  let progress = $derived.by(() => {
    const change = live.get(card.id);
    return change ? (change.played ? null : change.progress) : card.progress;
  });

  const percent = (p: number) => Math.round(p * 100);

  function enter(e: PointerEvent) {
    if (hovers) hover.enter(card.id, e);
  }

  /** An episode's show: its card's text leads to the show's page, while the artwork plays. */
  let series = $derived(
    card.kind === "Episode" && card.seriesId
      ? { id: card.seriesId, kind: "Series", title: card.title, meta: null, image: null, progress: null }
      : null,
  );

  function activate() {
    hover.close();
    markSource(shot);
    onActivate?.(card);
  }

  function openSeries() {
    if (!series) return;
    hover.close();
    markSource(shot);
    onActivate?.(series);
  }
</script>

{#snippet body()}
  {@render picture()}
  {@render text()}
{/snippet}

{#snippet text()}
  <span class="card-title" title={card.title}>{card.title}</span>
  {#if card.meta}<span class="card-meta" title={card.meta}>{card.meta}</span>{/if}
{/snippet}

{#snippet picture()}
  <span class="shot" bind:this={shot}>
    <Art image={card.image} title={card.title} sub={shape === "wide" ? null : card.meta} {width} />
    {#if saved}
      <span class="saved"><Icon name="download" size={12} /><span class="sr-only">Downloaded</span></span>
    {/if}
    {#if progress}
      <span
        class="progress"
        role="progressbar"
        aria-label="Watched"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent(progress)}
      >
        <span style:width="{percent(progress)}%"></span>
      </span>
    {/if}
  </span>
{/snippet}

{#if onActivate && series}
  <!-- An episode: the artwork plays it, the text opens its show. Two buttons, so both are reachable. -->
  <div
    class="card is-{shape} is-split"
    class:is-fluid={fluid}
    style:--w="{width}px"
    role="presentation"
    onpointerenter={enter}
    onpointerleave={hover.leave}
  >
    <button type="button" class="part is-picture" aria-label="Play {card.title}{card.meta ? `, ${card.meta}` : ''}" onclick={activate}>
      {@render picture()}
    </button>
    <button type="button" class="part is-text" aria-label="Open {card.title}" onclick={openSeries}>
      {@render text()}
    </button>
  </div>
{:else if onActivate}
  <button
    type="button"
    class="card is-{shape} is-link"
    class:is-fluid={fluid}
    style:--w="{width}px"
    onpointerenter={enter}
    onpointerleave={hover.leave}
    onclick={activate}
  >
    {@render body()}
  </button>
{:else}
  <div
    class="card is-{shape}"
    class:is-fluid={fluid}
    style:--w="{width}px"
    role="presentation"
    onpointerenter={enter}
    onpointerleave={hover.leave}
  >
    {@render body()}
  </div>
{/if}

{#if hover.key && shot}
  <HoverPanel
    anchor={shot}
    label={card.title}
    fit="cover"
    onEnter={enter}
    onLeave={hover.leave}
    onClose={hover.close}
    onActivate={onActivate
      ? () => {
          hover.close();
          markSource(shot);
          onActivate?.(card);
        }
      : undefined}
  >
    <TitleHover
      id={card.id}
      title={card.title}
      onPlay={(itemId) => {
        hover.close();
        markSource(shot);
        onPlay?.(itemId);
      }}
    />
  </HoverPanel>
{/if}

<style>
  .card {
    width: var(--w);
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .card.is-fluid {
    width: 100%;
  }
  button.card {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .shot :global(.art) {
    transition:
      transform var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized),
      box-shadow var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard);
  }
  .card.is-link:hover .shot :global(.art),
  .card.is-link:focus-visible .shot :global(.art) {
    transform: scale(1.04);
    box-shadow: var(--md-sys-elevation-3);
  }
  .card.is-link:active .shot :global(.art) {
    transform: scale(0.98);
    transition-duration: var(--md-sys-motion-duration-short);
  }
  .card.is-link:focus-visible {
    outline: none;
  }
  /* An episode's two parts: the artwork plays, the text opens the show and says so on hover. */
  .part {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .part:focus-visible {
    outline: none;
  }
  .part.is-picture:hover .shot :global(.art),
  .part.is-picture:focus-visible .shot :global(.art) {
    transform: scale(1.04);
    box-shadow: var(--md-sys-elevation-3);
  }
  .part.is-picture:active .shot :global(.art) {
    transform: scale(0.98);
  }
  .part.is-picture:focus-visible .shot {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: var(--r);
  }
  .part.is-text {
    align-self: flex-start;
    border-radius: 4px;
  }
  .part.is-text:hover .card-title,
  .part.is-text:focus-visible .card-title {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .part.is-text:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .card.is-link:focus-visible .shot {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: var(--r);
  }
  .shot {
    display: block;
    position: relative;
    width: 100%;
    margin-bottom: 8px;
  }
  .is-poster .shot {
    aspect-ratio: 2 / 3;
  }
  .is-wide .shot {
    aspect-ratio: 16 / 9;
  }
  .is-square .shot {
    aspect-ratio: 1;
  }
  .shot > :global(.art) {
    position: absolute;
    inset: 0;
  }
  .card-title,
  .card-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-title {
    font: 500 14px/20px var(--f-ui);
    letter-spacing: 0.1px;
    color: var(--md-sys-color-on-surface);
  }
  .card-meta {
    font: 400 12px/16px var(--f-ui);
    letter-spacing: 0.4px;
    color: var(--md-sys-color-on-surface-variant);
    font-variant-numeric: tabular-nums;
  }
  /* Downloaded: the download glyph in the complete green, on a dark plate so it reads on any art. */
  .saved {
    position: absolute;
    top: 6px;
    left: 6px;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--md-sys-shape-full);
    background: color-mix(in srgb, var(--md-sys-color-surface) 78%, transparent);
    backdrop-filter: blur(8px);
    color: var(--md-sys-color-primary);
  }
  .progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 4px;
    z-index: 2;
    overflow: hidden;
    background: color-mix(in srgb, var(--md-sys-color-surface) 60%, transparent);
  }
  .progress span {
    display: block;
    height: 100%;
    border-radius: 0 2px 2px 0;
    background: var(--md-sys-color-primary);
  }
</style>
