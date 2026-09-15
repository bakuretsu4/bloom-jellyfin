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
  .card.is-link:hover .shot :global(.art) {
    filter: brightness(1.07);
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
  .part.is-picture:hover .shot :global(.art) {
    filter: brightness(1.07);
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
    margin-bottom: 5px;
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
    font-size: 14px;
    font-weight: 500;
    line-height: 1.3;
  }
  .card-meta {
    font-size: 13px;
    line-height: 1.3;
    color: var(--ink-3);
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
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: rgba(18, 20, 19, 0.72);
    color: var(--good);
  }
  .progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    z-index: 2;
    overflow: hidden;
    border-radius: 0 0 var(--r) var(--r);
    background: rgba(18, 20, 19, 0.4);
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--accent-media);
  }
</style>
