<script lang="ts">
  import * as api from "./api";
  import Art from "./Art.svelte";
  import Icon from "./Icon.svelte";

  let {
    query = $bindable(""),
    onOpen,
    onSearch,
    onSignedOut,
  }: {
    query?: string;
    /** A suggestion was chosen. */
    onOpen: (card: api.Card) => void;
    /** The full results page for a query; an empty query leaves it. */
    onSearch: (query: string) => void;
    onSignedOut: () => void;
  } = $props();

  const SUGGESTIONS = 8;
  const KINDS: Record<string, string> = { Movie: "Film", Series: "Show", Episode: "Episode", Video: "Video" };

  let input = $state<HTMLInputElement>();
  let results = $state<api.Card[]>([]);
  /** The query `results` answer. */
  let answered = $state("");
  let open = $state(false);
  /** The highlighted row: a suggestion, or `results.length` for "See all results". */
  let active = $state(-1);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;

  let trimmed = $derived(query.trim());
  let showList = $derived(open && trimmed.length >= 2);
  let pending = $derived(answered !== trimmed);

  /** For the `/` shortcut. */
  export function focus() {
    input?.focus();
    input?.select();
  }

  function onInput() {
    open = true;
    active = -1;
    clearTimeout(timer);
    if (trimmed.length < 2) {
      results = [];
      answered = trimmed;
      return;
    }
    const wanted = trimmed;
    timer = setTimeout(() => void lookUp(wanted), 180);
  }

  async function lookUp(wanted: string) {
    const mine = ++request;
    try {
      const found = await api.search(wanted, SUGGESTIONS);
      if (mine !== request) return;
      results = found;
      answered = wanted;
    } catch (e) {
      if (api.isSignedOut(e)) onSignedOut();
      else if (mine === request) {
        results = [];
        answered = wanted;
      }
    }
  }

  function close() {
    open = false;
    active = -1;
  }

  function choose(index: number) {
    if (index >= 0 && index < results.length) {
      const card = results[index];
      close();
      input?.blur();
      onOpen(card);
    } else {
      seeAll();
    }
  }

  function seeAll() {
    if (trimmed.length < 2) return;
    close();
    onSearch(trimmed);
  }

  function onKey(e: KeyboardEvent) {
    const rows = results.length + 1;
    switch (e.key) {
      case "ArrowDown":
        if (!showList) open = true;
        else active = (active + 1) % rows;
        break;
      case "ArrowUp":
        if (!showList) return;
        active = active <= 0 ? rows - 1 : active - 1;
        break;
      case "Enter":
        if (showList && active >= 0) choose(active);
        else seeAll();
        break;
      case "Escape":
        if (showList) close();
        else {
          query = "";
          results = [];
          input?.blur();
          onSearch("");
        }
        break;
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<div
  class="search-box"
  onfocusout={(e) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node | null)) close();
  }}
>
  <div class="search" role="search">
    <Icon name="search" size={16} />
    <label for="search" class="sr-only">Search films, shows and episodes</label>
    <input
      id="search"
      type="search"
      placeholder="Search films, shows and episodes"
      autocomplete="off"
      spellcheck="false"
      role="combobox"
      aria-expanded={showList}
      aria-controls="search-suggestions"
      aria-autocomplete="list"
      aria-activedescendant={showList && active >= 0 ? `search-option-${active}` : undefined}
      bind:this={input}
      bind:value={query}
      onfocus={() => (open = true)}
      oninput={onInput}
      onkeydown={onKey}
    />
  </div>

  {#if showList}
    <!-- Rows keep the input focused (mousedown is cancelled), so keys go on working. -->
    <ul class="suggestions" id="search-suggestions" role="listbox" aria-label="Suggestions" aria-busy={pending}>
      {#each results as card, i (card.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events: the search box handles keys -->
        <li
          id="search-option-{i}"
          role="option"
          aria-selected={i === active}
          class:is-active={i === active}
          onpointermove={() => (active = i)}
          onmousedown={(e) => e.preventDefault()}
          onclick={() => choose(i)}
        >
          <span class="thumb"><Art image={card.image} title={card.title} width={40} /></span>
          <span class="text">
            <span class="name">{card.title}</span>
            <span class="meta">{[KINDS[card.kind] ?? card.kind, card.meta].filter(Boolean).join(", ")}</span>
          </span>
        </li>
      {/each}
      {#if !pending && results.length === 0}
        <li class="note" role="presentation">Nothing called “{trimmed}”</li>
      {:else if pending && results.length === 0}
        <li class="note" role="presentation">Searching</li>
      {/if}
      <!-- svelte-ignore a11y_click_events_have_key_events: the search box handles keys -->
      <li
        id="search-option-{results.length}"
        class="all"
        role="option"
        aria-selected={active === results.length}
        class:is-active={active === results.length}
        onpointermove={() => (active = results.length)}
        onmousedown={(e) => e.preventDefault()}
        onclick={seeAll}
      >
        See all results for “{trimmed}”
      </li>
    </ul>
  {/if}
</div>

<style>
  .search-box {
    position: relative;
    flex: 1 1 auto;
    max-width: 520px;
    margin-inline: auto;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    color: var(--ink-3);
    background: var(--ground);
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    transition: border-color 0.2s var(--ease);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    width: 100%;
    border: 0;
    outline: none;
    background: none;
    color: var(--ink);
    font-size: 14px;
  }
  .search input::placeholder {
    color: var(--ink-3);
  }

  .suggestions {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 6px);
    z-index: 30;
    max-height: min(70vh, 560px);
    overflow-y: auto;
    margin: 0;
    padding: 6px;
    list-style: none;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: var(--r);
    box-shadow: var(--shadow);
    animation: pop 0.18s var(--ease) both;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 6px 8px;
    border-radius: var(--r-ctl);
    color: var(--ink-2);
    cursor: pointer;
  }
  li.is-active {
    background: var(--surface-2);
    color: var(--ink);
  }
  .thumb {
    position: relative;
    flex: none;
    width: 34px;
    aspect-ratio: 2 / 3;
  }
  .thumb > :global(.art) {
    position: absolute;
    inset: 0;
    border-radius: 4px;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }
  .name,
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-size: 13.5px;
    font-weight: 500;
    color: var(--ink);
  }
  .meta {
    font-size: 12px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .note {
    padding: 10px 8px;
    font-size: 13px;
    color: var(--ink-3);
    cursor: default;
  }
  .all {
    margin-top: 4px;
    padding-block: 9px;
    border-top: 1px solid var(--line-soft);
    border-radius: 0 0 var(--r-ctl) var(--r-ctl);
    font-size: 13px;
    font-weight: 500;
    color: var(--accent);
  }
  .all.is-active {
    color: var(--accent);
  }
</style>
