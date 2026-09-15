<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "./api";
  import MediaCard from "./MediaCard.svelte";

  let {
    query,
    onOpen,
    onPlay,
    onClear,
    onSignedOut,
  }: {
    query: string;
    onOpen: (card: api.Card) => void;
    onPlay: (itemId: string) => void;
    onClear: () => void;
    onSignedOut: () => void;
  } = $props();

  let results = $state<api.Card[] | null>(null);
  let error = $state("");
  let grid = $state<HTMLDivElement>();

  /** Arrow keys move between results by the grid's own columns; Home and End jump to the ends. */
  function onGridKey(e: KeyboardEvent) {
    if (!grid || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(e.key)) return;
    const cards = Array.from(grid.querySelectorAll<HTMLElement>(":scope > .card"));
    const index = cards.indexOf(document.activeElement as HTMLElement);
    if (index < 0) return;
    const columns = getComputedStyle(grid).gridTemplateColumns.split(" ").filter(Boolean).length || 1;
    const target: Record<string, number> = {
      ArrowLeft: index - 1,
      ArrowRight: index + 1,
      ArrowUp: index - columns,
      ArrowDown: index + columns,
      Home: 0,
      End: cards.length - 1,
    };
    const next = cards[target[e.key]];
    if (!next) return;
    e.preventDefault();
    next.focus();
    next.scrollIntoView({ block: "nearest" });
  }

  async function load() {
    error = "";
    try {
      results = await api.search(query);
    } catch (e) {
      if (api.isSignedOut(e)) onSignedOut();
      else error = api.message(e);
    }
  }

  $effect(() => {
    untrack(load);
  });
</script>

<div class="page">
  <header class="page-head">
    <div class="page-heading">
      <h1 class="page-title">Results for “{query}”</h1>
      {#if results?.length}
        <span class="eyebrow">
          {results.length === api.GRID_PAGE ? `First ${results.length}` : results.length}
          {results.length === 1 ? "result" : "results"}
        </span>
      {/if}
    </div>
  </header>

  {#if error}
    <div class="page-notice" role="alert">
      <h2>Search didn't work</h2>
      <p>{error}</p>
      <button class="btn" onclick={load}>Try again</button>
    </div>
  {:else if !results}
    <div class="media-grid skel" aria-busy="true" aria-label="Searching">
      {#each Array.from({ length: 6 }, (_, i) => i) as i (i)}<div class="sk-card"></div>{/each}
    </div>
  {:else if results.length === 0}
    <div class="empty">
      <div class="empty-art" aria-hidden="true"><span></span><span></span><span></span></div>
      <h2>No matches</h2>
      <p>
        Nothing in your libraries is called “{query}”. Check the spelling, or search for a film, show or
        episode title.
      </p>
      <button class="btn" onclick={onClear}>Clear search</button>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions: the keys move focus between the cards inside -->
    <div class="media-grid stagger" bind:this={grid} onkeydown={onGridKey}>
      {#each results as card (card.id)}
        <MediaCard {card} shape="poster" width={280} fluid onActivate={onOpen} {onPlay} />
      {/each}
    </div>
  {/if}
</div>
