<script lang="ts">
  import Icon from "./Icon.svelte";
  import { airDate } from "./hover.svelte";

  let {
    seriesTitle = null,
    code = null,
    title,
    premiereDate = null,
    overview = null,
    played,
    resume = false,
    playing = false,
    onPlay,
    onTogglePlayed,
  }: {
    seriesTitle?: string | null;
    /** "E4" or "S1 E4". */
    code?: string | null;
    title: string;
    premiereDate?: string | null;
    overview?: string | null;
    played: boolean;
    /** Partly watched. */
    resume?: boolean;
    /** Already playing on this screen. */
    playing?: boolean;
    onPlay: () => void;
    onTogglePlayed: () => void;
  } = $props();

  let menuOpen = $state(false);
  let date = $derived(airDate(premiereDate));
  // "E4" for the button, even when the card says "S1 E4".
  let episode = $derived(code?.split(" ").pop() ?? "");
  let verb = $derived(resume ? "Continue" : played ? "Watch again" : "Play");
</script>

<!-- The contents of a hover card for one episode. -->
{#if seriesTitle}<span class="series">{seriesTitle}</span>{/if}
<span class="title">{#if code}<span class="code">{code}</span>{/if}<span>{title}</span></span>
{#if date || played}
  <span class="meta">
    {#if date}<span>Aired {date}</span>{/if}
    {#if played}
      {#if date}<span class="sep"></span>{/if}<span class="watched"><Icon name="check" size={12} />Watched</span>
    {/if}
  </span>
{/if}
{#if overview}<p class="overview">{overview}</p>{/if}

<div class="foot">
  {#if playing}
    <span class="now">Now playing</span>
  {:else}
    <button class="play" onclick={onPlay}><Icon name="play" size={14} />{verb}{episode ? ` ${episode}` : ""}</button>
  {/if}
  <div class="menu-anchor">
    <button
      class="more"
      aria-label="More for {code ?? title}"
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      onclick={() => (menuOpen = !menuOpen)}
    >
      <Icon name="more" size={16} />
    </button>
    {#if menuOpen}
      <div class="menu" role="menu" aria-label={code ?? title}>
        <button
          class="menu-item"
          role="menuitem"
          onclick={() => {
            menuOpen = false;
            onTogglePlayed();
          }}
        >
          {played ? "Mark as unwatched" : "Mark as watched"}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .series {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 500;
    color: var(--ink-3);
  }
  .title {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    flex: none;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.3;
  }
  .code {
    margin-right: 7px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  /* Laid over a tile: the synopsis takes the room that's left and fades out where it ends. */
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
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .sep {
    flex: none;
    width: 1px;
    height: 11px;
    background: var(--line);
  }
  .watched {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--good);
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
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: auto;
    padding-top: 4px;
  }
  .play {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    margin-left: -8px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--accent);
    font: inherit;
    font-size: 13.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .play:hover {
    background: var(--surface-2);
  }
  .now {
    font-size: 13px;
    color: var(--accent-media);
  }
  .menu-anchor {
    position: relative;
  }
  .more {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--ink-2);
    cursor: pointer;
  }
  .more:hover,
  .more[aria-expanded="true"] {
    background: var(--surface-2);
    color: var(--ink);
  }
  /* Upwards, so a card laid over its tile doesn't cut it off. */
  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 4px);
    z-index: 1;
    min-width: 180px;
    padding: 6px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: var(--r);
    box-shadow: var(--shadow);
  }
  .menu-item {
    display: block;
    width: 100%;
    padding: 7px 8px;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--ink-2);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .menu-item:hover,
  .menu-item:focus-visible {
    outline: none;
    background: var(--surface-2);
    color: var(--ink);
  }
</style>
