<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "./api";
  import { downloads } from "./downloads.svelte";
  import MediaCard from "./MediaCard.svelte";
  import Rail from "./Rail.svelte";
  import Spotlight from "./Spotlight.svelte";

  let {
    onSignedOut,
    onOpen,
    onPlay,
    refreshToken = 0,
  }: {
    onSignedOut: () => void;
    /** A card was chosen: a library, a show or film (opens its page), or an episode (plays). */
    onOpen: (card: api.Card) => void;
    onPlay: (itemId: string) => void;
    /** Bumped by the app after playback, so Continue watching and Next up reload. */
    refreshToken?: number;
  } = $props();

  // Albums wait for music playback; everything else here leads somewhere.
  const OPENABLE = new Set(["Movie", "Series", "Episode", "Video", "MusicVideo", "CollectionFolder", "UserView"]);

  let spotlight = $state<api.Spotlight[] | null>(null);
  let sections = $state<api.Section[] | null>(null);
  let error = $state("");
  let loading = $state(false);

  // Rendered widths, also used to ask Rust for artwork at the right size.
  const WIDTH: Record<api.Shape, number> = { poster: 240, wide: 360, square: 200 };

  async function load() {
    loading = true;
    error = "";
    try {
      sections = await api.home();
    } catch (e) {
      if (api.isSignedOut(e)) return onSignedOut();
      error = api.message(e);
    } finally {
      loading = false;
    }
  }

  // Separate from the rails so a slow random pick never holds them up. Failing is quiet: the
  // spotlight is a nicety, and the rails still carry the screen.
  async function loadSpotlight() {
    try {
      spotlight = await api.spotlight();
    } catch (e) {
      if (api.isSignedOut(e)) return onSignedOut();
      spotlight = [];
    }
  }

  $effect(() => {
    untrack(loadSpotlight);
  });

  // The Downloaded row: read from disk, so it's there with no server too, and read again as
  // downloads finish or go.
  let downloaded = $state<api.Card[]>([]);
  let finishedKey = $derived(downloads.list.filter((d) => d.status === "done").map((d) => d.id).join());
  $effect(() => {
    void finishedKey;
    untrack(() =>
      api
        .downloadedTitles()
        .then((cards) => (downloaded = cards))
        .catch(() => {}),
    );
  });
  const downloadedRow = (cards: api.Card[]): api.Section => ({ id: "downloaded", title: "Downloaded", shape: "poster", cards });

  /** The server's rows, with Downloaded after Continue watching and Next up. */
  let rows = $derived.by(() => {
    if (!sections || !downloaded.length) return sections;
    const latest = sections.findIndex((s) => s.id.startsWith("latest-"));
    const at = latest < 0 ? sections.length : latest;
    return [...sections.slice(0, at), downloadedRow(downloaded), ...sections.slice(at)];
  });

  // On first show, and again whenever the app says playback changed what's in progress. The rows
  // already on screen stay while the new ones load.
  $effect(() => {
    void refreshToken;
    untrack(load);
  });
</script>

{#if spotlight === null && !error}
  <div class="spot-placeholder" aria-hidden="true"></div>
{:else if spotlight?.length && !error}
  <Spotlight items={spotlight} {onSignedOut} {onPlay} {onOpen} />
{/if}

<div class="home">
  {#if error}
    <div class="notice" role="alert">
      <h1>Couldn't load your home screen</h1>
      <p>{error}</p>
      <button class="btn" onclick={load} disabled={loading}>{loading ? "Trying again" : "Try again"}</button>
    </div>
    {#if downloaded.length}
      <div class="rails stagger is-offline">{@render rail(downloadedRow(downloaded))}</div>
    {/if}
  {:else if !rows}
    <div class="rails skel" aria-busy="true" aria-label="Loading your libraries">
      {#each [0, 1, 2] as row (row)}
        <div class="sk-rail">
          <div class="sk-head"></div>
          <div class="sk-row">
            {#each Array.from({ length: 9 }, (_, i) => i) as i (i)}
              <div class="sk-card" class:is-wide={row === 0}></div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {:else if rows.length === 0}
    <div class="notice">
      <h1>Nothing here yet</h1>
      <p>
        This account can't see any libraries. An administrator can give it access in the Jellyfin
        dashboard, under Users.
      </p>
    </div>
  {:else}
    <div class="rails stagger">
      {#each rows as section (section.id)}{@render rail(section)}{/each}
    </div>
  {/if}
</div>

{#snippet rail(section: api.Section)}
  <Rail title={section.title}>
    {#each section.cards as card (card.id)}
      <MediaCard
        {card}
        shape={section.shape}
        width={WIDTH[section.shape]}
        onActivate={OPENABLE.has(card.kind) ? onOpen : undefined}
        {onPlay}
      />
    {/each}
  </Rail>
{/snippet}

<style>
  .home {
    padding-inline: clamp(16px, 2.2vw, 26px);
    padding-block: 22px 56px;
  }
  /* Holds the spotlight's height while it loads, so the rails don't jump down. */
  .spot-placeholder {
    height: clamp(360px, 54vh, 580px);
    margin: 8px clamp(16px, 2.2vw, 26px) 0;
    border-radius: var(--md-sys-shape-xl);
    background: var(--md-sys-color-surface-container);
  }
  .rails {
    display: flex;
    flex-direction: column;
    gap: 30px;
  }

  .notice {
    max-width: 460px;
    padding-top: 12vh;
  }
  /* With no server, the Downloaded row sits under the notice. */
  .rails.is-offline {
    margin-top: 36px;
  }
  .notice h1 {
    margin: 0 0 6px;
    font-stretch: 118%;
    font-weight: 600;
    letter-spacing: -0.015em;
    font-size: 1.4rem;
    line-height: 1.15;
  }
  .notice p {
    margin: 0 0 16px;
    color: var(--ink-2);
    font-size: 14px;
  }

  /* Placeholders shaped like the rails; the scan line passing over them is `.skel` in app.css. */
  .sk-head {
    width: 180px;
    height: 14px;
    margin: 3px 0 15px;
    border-radius: var(--md-sys-shape-full);
    background: var(--md-sys-color-surface-container-high);
  }
  .sk-row {
    display: flex;
    gap: 12px;
    overflow: hidden;
  }
  .sk-card {
    flex: none;
    width: 240px;
    aspect-ratio: 2 / 3;
    border-radius: var(--md-sys-shape-lg);
    background: var(--md-sys-color-surface-container);
  }
  .sk-card.is-wide {
    width: 360px;
    aspect-ratio: 16 / 9;
  }
</style>
