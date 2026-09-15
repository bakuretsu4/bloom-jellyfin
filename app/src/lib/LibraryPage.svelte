<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "./api";
  import Icon from "./Icon.svelte";
  import { downloadedTitleIds } from "./downloads.svelte";
  import MediaCard from "./MediaCard.svelte";
  import Pager from "./Pager.svelte";
  import Select from "./Select.svelte";

  let {
    id,
    title,
    onBack,
    onOpen,
    onPlay,
    onSignedOut,
  }: {
    id: string;
    title: string;
    onBack: () => void;
    onOpen: (card: api.Card) => void;
    onPlay: (itemId: string) => void;
    onSignedOut: () => void;
  } = $props();

  const SORTS: { value: api.LibrarySort; label: string }[] = [
    { value: "added", label: "Date added" },
    { value: "name", label: "Name" },
    { value: "year", label: "Year" },
    { value: "rating", label: "Rating" },
  ];

  let sort = $state<api.LibrarySort>("added");
  /** Only the films and shows with something downloaded. */
  let onlyDownloaded = $state(false);
  let page = $state(0);
  let grid = $state<api.Grid | null>(null);
  let error = $state("");
  let loading = $state(false);
  let head = $state<HTMLElement>();

  let pages = $derived(grid ? Math.max(1, Math.ceil(grid.total / api.GRID_PAGE)) : 1);
  let request = 0;

  async function load() {
    const mine = ++request;
    loading = true;
    error = "";
    try {
      const ids = onlyDownloaded ? downloadedTitleIds() : undefined;
      const result = await api.libraryItems(id, sort, page * api.GRID_PAGE, ids);
      if (mine === request) grid = result;
    } catch (e) {
      if (mine !== request) return;
      if (api.isSignedOut(e)) return onSignedOut();
      // No server: every downloaded title, since which library each belongs to needs the server.
      const cards = onlyDownloaded ? await api.downloadedTitles().catch(() => []) : [];
      if (mine !== request) return;
      if (cards.length) grid = { total: cards.length, shape: "poster", cards };
      else error = api.message(e);
    } finally {
      if (mine === request) loading = false;
    }
  }

  $effect(() => {
    void sort;
    void page;
    void onlyDownloaded;
    untrack(load);
  });

  function goToPage(next: number) {
    page = next;
    head?.scrollIntoView({ block: "start" });
  }
</script>

<div class="page">
  <button class="btn back" onclick={onBack}><Icon name="chevl" size={14} />Back</button>

  <header class="page-head" bind:this={head}>
    <div class="page-heading">
      <h1 class="page-title">{title}</h1>
      {#if grid}
        <span class="eyebrow">
          {grid.total} {grid.total === 1 ? "title" : "titles"}{pages > 1 ? `, page ${page + 1} of ${pages}` : ""}
        </span>
      {/if}
    </div>
    <div class="head-tools">
      <button
        class="btn"
        aria-pressed={onlyDownloaded}
        onclick={() => {
          onlyDownloaded = !onlyDownloaded;
          page = 0;
        }}
      >
        <Icon name="download" size={14} />Downloaded
      </button>
      <Select
        label="Sort by"
        value={sort}
        options={SORTS}
        onChange={(next) => {
          sort = next;
          page = 0;
        }}
      />
    </div>
  </header>

  {#if error}
    <div class="page-notice" role="alert">
      <h2>Couldn't load this library</h2>
      <p>{error}</p>
      <button class="btn" onclick={load}>Try again</button>
    </div>
  {:else if !grid}
    <div class="media-grid skel" aria-busy="true" aria-label="Loading {title}">
      {#each Array.from({ length: 12 }, (_, i) => i) as i (i)}<div class="sk-card"></div>{/each}
    </div>
  {:else if grid.cards.length === 0 && onlyDownloaded}
    <div class="page-notice">
      <h2>Nothing from {title} downloaded</h2>
      <p>Download a film or an episode from its page, and it shows here.</p>
    </div>
  {:else if grid.cards.length === 0}
    <div class="page-notice">
      <h2>Nothing in {title} yet</h2>
      <p>The library is empty, or it hasn't finished scanning on the server.</p>
    </div>
  {:else}
    <div class="media-grid stagger" class:is-wide={grid.shape === "wide"} aria-busy={loading}>
      {#each grid.cards as card (card.id)}
        <MediaCard {card} shape={grid.shape} width={grid.shape === "wide" ? 420 : 280} fluid onActivate={onOpen} {onPlay} />
      {/each}
    </div>
    <Pager {page} {pages} onPage={goToPage} label="{title} pages" />
  {/if}
</div>

<style>
  .head-tools {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  /* Pressed while the filter is on, as the title page's toggles are. */
  .head-tools .btn[aria-pressed="true"] {
    color: var(--accent);
    border-color: color-mix(in oklab, var(--accent) 42%, var(--line));
  }
</style>
