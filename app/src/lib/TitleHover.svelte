<script lang="ts">
  import * as api from "./api";
  import EpisodeHover from "./EpisodeHover.svelte";
  import Icon from "./Icon.svelte";
  import { cardDetail, forgetCardDetail, runtime } from "./hover.svelte";
  import { forgetLive } from "./live.svelte";

  let {
    id,
    title,
    onPlay,
  }: {
    id: string;
    /** Shown while the detail loads. */
    title: string;
    onPlay: (itemId: string) => void;
  } = $props();

  let detail = $state<api.CardDetail | null>(null);
  let failed = $state(false);
  let busy = $state(false);

  $effect(() => {
    const wanted = id;
    cardDetail(wanted)
      .then((loaded) => {
        if (wanted === id) detail = loaded;
      })
      .catch(() => (failed = true));
  });

  let meta = $derived.by(() => {
    const d = detail;
    if (!d) return [];
    const parts: string[] = [];
    if (d.communityRating) parts.push(`${d.communityRating.toFixed(1)}/10`);
    if (d.kind === "Series") {
      if (d.year) {
        if (d.status === "Continuing") parts.push(`${d.year}–`);
        else if (d.endYear && d.endYear !== d.year) parts.push(`${d.year}–${d.endYear}`);
        else parts.push(String(d.year));
      }
      if (d.seasonCount) parts.push(`${d.seasonCount} ${d.seasonCount === 1 ? "season" : "seasons"}`);
      if (d.episodeCount) parts.push(`${d.episodeCount} ${d.episodeCount === 1 ? "episode" : "episodes"}`);
    } else {
      if (d.year) parts.push(String(d.year));
      if (d.runtimeMinutes) parts.push(runtime(d.runtimeMinutes));
    }
    return parts;
  });

  async function toggle(kind: "favorite" | "played") {
    const d = detail;
    if (!d || busy) return;
    busy = true;
    try {
      if (kind === "favorite") d.favorite = await api.setFavorite(d.id, !d.favorite);
      else d.played = await api.setPlayed(d.id, !d.played);
      if (d.played) d.resume = false;
      forgetCardDetail(d.id);
      // A change made here wins over an older one heard from the server.
      forgetLive(d.id);
    } catch {
      // A hover card stays quiet; the title's page reports failures.
    } finally {
      busy = false;
    }
  }
</script>

<!-- The contents of a hover card for a film or a show, or an episode on Home. -->
{#if detail?.kind === "Episode"}
  <EpisodeHover
    seriesTitle={detail.seriesTitle}
    code={detail.code}
    title={detail.title}
    premiereDate={detail.premiereDate}
    overview={detail.overview}
    played={detail.played}
    resume={detail.resume}
    onPlay={() => onPlay(id)}
    onTogglePlayed={() => toggle("played")}
  />
{:else}
  <span class="head">
    <span class="title">{detail?.title ?? title}</span>
    {#if detail?.favorite}
      <span class="favorited" role="img" aria-label="In your favorites" title="In your favorites"><Icon name="heart" size={14} /></span>
    {/if}
  </span>
  {#if detail}
    {#if meta.length || detail.officialRating}
      <span class="meta">
        {#each meta as part, i (i)}{#if i > 0}<span class="sep"></span>{/if}<span>{part}</span>{/each}
        {#if detail.officialRating}<span class="tag">{detail.officialRating}</span>{/if}
      </span>
    {/if}
    {#if detail.overview}<p class="overview">{detail.overview}</p>{/if}
    <div class="actions">
      <button class="btn btn-primary play" aria-label={detail.resume ? "Resume" : "Play"} onclick={() => onPlay(id)}>
        <Icon name="play" size={14} /><span class="play-label">{detail.resume ? "Resume" : "Play"}</span>
      </button>
      <button
        class="toggle"
        aria-pressed={detail.favorite}
        aria-label={detail.favorite ? "Remove from favorites" : "Add to favorites"}
        title={detail.favorite ? "Remove from favorites" : "Add to favorites"}
        disabled={busy}
        onclick={() => toggle("favorite")}
      >
        <Icon name="heart" size={16} />
      </button>
      <button
        class="toggle"
        aria-pressed={detail.played}
        aria-label={detail.played ? "Mark as unwatched" : "Mark as watched"}
        title={detail.played ? "Mark as unwatched" : "Mark as watched"}
        disabled={busy}
        onclick={() => toggle("played")}
      >
        <Icon name="check" size={16} />
      </button>
    </div>
  {:else if failed}
    <p class="overview">Couldn't load the details.</p>
  {:else}
    <span class="sk skel" aria-busy="true" aria-label="Loading"><span></span><span></span><span></span></span>
  {/if}
{/if}

<style>
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }
  .title {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.3;
  }
  .head {
    flex: none;
  }
  /* Laid over a poster: the synopsis takes the room that's left and fades out where it ends. */
  :global(.hover-card.is-cover) .title {
    font-size: 14px;
  }
  :global(.hover-card.is-cover) .overview {
    flex: 1 1 0;
    min-height: 0;
    display: block;
    font-size: 12.5px;
    line-height: 1.5;
    -webkit-mask-image: linear-gradient(to bottom, #000 calc(100% - 1.5em), transparent);
    mask-image: linear-gradient(to bottom, #000 calc(100% - 1.5em), transparent);
  }
  /* A rail's poster: Play keeps its icon and name for screen readers, and loses the word. */
  @container (max-width: 210px) {
    .actions .play {
      width: 32px;
      padding: 0;
      justify-content: center;
    }
    .play-label {
      display: none;
    }
    .meta {
      gap: 6px;
      font-size: 11.5px;
    }
  }
  .favorited {
    flex: none;
    display: grid;
    margin-top: 2px;
    color: var(--accent);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }
  .sep {
    flex: none;
    width: 1px;
    height: 11px;
    background: var(--line);
  }
  .tag {
    padding: 1px 6px;
    border: 1px solid var(--line);
    border-radius: 5px;
    font-size: 11px;
    white-space: nowrap;
  }
  .overview {
    margin: 0;
    white-space: pre-line;
    display: -webkit-box;
    -webkit-line-clamp: 6;
    line-clamp: 6;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 13.5px;
    line-height: 1.55;
    color: var(--ink-2);
  }
  .actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: auto;
    padding-top: 4px;
  }
  .actions .btn {
    height: 32px;
    padding: 0 12px;
    font-size: 13px;
  }
  .toggle {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--surface);
    color: var(--ink-2);
    cursor: pointer;
    transition: background 0.16s var(--ease), color 0.16s var(--ease), border-color 0.16s var(--ease);
  }
  .toggle:hover {
    background: var(--surface-2);
    color: var(--ink);
  }
  .toggle[aria-pressed="true"] {
    color: var(--accent);
    border-color: color-mix(in oklab, var(--accent) 42%, var(--line));
  }
  .toggle:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .sk {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sk span {
    height: 11px;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .sk span:first-child {
    width: 55%;
  }
  .sk span:last-child {
    width: 80%;
  }
</style>
