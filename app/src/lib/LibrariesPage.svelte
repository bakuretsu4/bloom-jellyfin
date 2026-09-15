<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "./api";
  import MediaCard from "./MediaCard.svelte";

  let { onOpen, onSignedOut }: { onOpen: (card: api.Card) => void; onSignedOut: () => void } = $props();

  let cards = $state<api.Card[] | null>(null);
  let error = $state("");

  async function load() {
    error = "";
    try {
      cards = await api.libraries();
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
      <h1 class="page-title">Libraries</h1>
      {#if cards?.length}<span class="eyebrow">{cards.length} {cards.length === 1 ? "library" : "libraries"}</span>{/if}
    </div>
  </header>

  {#if error}
    <div class="page-notice" role="alert">
      <h2>Couldn't load your libraries</h2>
      <p>{error}</p>
      <button class="btn" onclick={load}>Try again</button>
    </div>
  {:else if !cards}
    <div class="media-grid is-wide" aria-busy="true" aria-label="Loading libraries">
      {#each [0, 1, 2, 3] as i (i)}<div class="sk-card is-wide"></div>{/each}
    </div>
  {:else if cards.length === 0}
    <div class="page-notice">
      <h2>No libraries</h2>
      <p>This account can't see any libraries. An administrator can give it access in the Jellyfin dashboard, under Users.</p>
    </div>
  {:else}
    <div class="media-grid is-wide">
      {#each cards as card (card.id)}
        <MediaCard {card} shape="wide" width={320} fluid onActivate={onOpen} />
      {/each}
    </div>
  {/if}
</div>
