<script lang="ts" module>
  export type Place = "home" | "libraries" | "downloads" | "settings" | "servers";

  /** How well a row's label (and extra words) matches what was typed: -1 not at all. */
  function score(label: string, words: string, typed: string): number {
    const name = label.toLowerCase();
    if (name.startsWith(typed)) return 4;
    if (name.split(/\s+/).some((word) => word.startsWith(typed))) return 3;
    if (name.includes(typed)) return 2;
    if (words.split(/\s+/).some((word) => word.startsWith(typed))) return 1;
    return -1;
  }
</script>

<script lang="ts">
  import { tick, type ComponentProps } from "svelte";
  import * as api from "./api";
  import Art from "./Art.svelte";
  import Icon from "./Icon.svelte";

  type IconName = ComponentProps<typeof Icon>["name"];

  let {
    onClose,
    onOpenTitle,
    onGo,
    onLibrary,
    onSettings,
    onShortcuts,
    onSignedOut,
  }: {
    onClose: () => void;
    /** A film, show, collection or episode was chosen. */
    onOpenTitle: (card: api.Card) => void;
    onGo: (place: Place) => void;
    onLibrary: (library: api.Card) => void;
    /** A Settings section, by its heading's id. */
    onSettings: (sectionId: string) => void;
    onShortcuts: () => void;
    onSignedOut: () => void;
  } = $props();

  type Row = {
    key: string;
    group: "Go to" | "Libraries" | "Settings" | "Titles";
    label: string;
    detail: string | null;
    icon: IconName | null;
    card: api.Card | null;
    run: () => void;
  };

  const SUGGESTIONS = 8;
  const KINDS: Record<string, string> = { Movie: "Film", Series: "Show", Episode: "Episode", Video: "Video", BoxSet: "Collection" };

  const PLACES: { place: Place; label: string; icon: IconName; words: string }[] = [
    { place: "home", label: "Home", icon: "home", words: "start continue watching next up spotlight" },
    { place: "libraries", label: "Libraries", icon: "library", words: "library films shows browse" },
    { place: "downloads", label: "Downloads", icon: "download", words: "offline saved storage" },
    { place: "settings", label: "Settings", icon: "gear", words: "preferences options" },
    { place: "servers", label: "Servers", icon: "user", words: "server accounts sign in jellyfin" },
  ];
  // Headings in SettingsPage.svelte, with the words someone might type looking for them.
  const SECTIONS: { id: string; label: string; words: string }[] = [
    { id: "set-playback", label: "Playback", words: "decoder hardware quality direct play audio subtitles language autoplay" },
    { id: "set-notifications", label: "Notifications", words: "notify new episodes films alerts added" },
    { id: "set-discord", label: "Discord", words: "presence rich status profile" },
    { id: "set-downloads", label: "Downloads", words: "location folder quality network at once parallel" },
    { id: "set-appearance", label: "Appearance", words: "theme dark light accent colour color zoom reduce motion" },
    { id: "set-about", label: "About", words: "version server graphics player" },
  ];

  let input = $state<HTMLInputElement>();
  let list = $state<HTMLUListElement>();
  let query = $state("");
  let libraries = $state<api.Card[]>([]);
  let titles = $state<api.Card[]>([]);
  /** The query `titles` answer. */
  let answered = $state("");
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let request = 0;
  const opener = document.activeElement as HTMLElement | null;

  let typed = $derived(query.trim().toLowerCase());
  let pending = $derived(typed.length >= 2 && answered !== query.trim());

  $effect(() => {
    tick().then(() => input?.focus());
    api
      .libraries()
      .then((found) => (libraries = found))
      .catch(() => {});
    return () => {
      clearTimeout(timer);
      opener?.focus?.();
    };
  });

  let rows = $derived.by(() => {
    const ranked = <T,>(items: T[], label: (item: T) => string, words: (item: T) => string) =>
      typed
        ? items
            .map((item) => ({ item, rank: score(label(item), words(item), typed) }))
            .filter((entry) => entry.rank >= 0)
            .sort((a, b) => b.rank - a.rank)
            .map((entry) => entry.item)
        : items;
    const out: Row[] = [];
    for (const p of ranked(PLACES, (p) => p.label, (p) => p.words)) {
      out.push({ key: `go-${p.place}`, group: "Go to", label: p.label, detail: null, icon: p.icon, card: null, run: () => onGo(p.place) });
    }
    if (!typed || score("Keyboard shortcuts", "keys help", typed) >= 0) {
      out.push({ key: "go-shortcuts", group: "Go to", label: "Keyboard shortcuts", detail: null, icon: "help", card: null, run: onShortcuts });
    }
    for (const library of ranked(libraries, (l) => l.title, () => "library")) {
      out.push({ key: `lib-${library.id}`, group: "Libraries", label: library.title, detail: null, icon: "library", card: null, run: () => onLibrary(library) });
    }
    if (typed) {
      for (const section of ranked(SECTIONS, (s) => s.label, (s) => s.words)) {
        out.push({ key: `set-${section.id}`, group: "Settings", label: section.label, detail: "Settings", icon: "gear", card: null, run: () => onSettings(section.id) });
      }
      for (const card of titles) {
        const detail = [KINDS[card.kind] ?? card.kind, card.meta].filter(Boolean).join(", ");
        out.push({ key: `title-${card.id}`, group: "Titles", label: card.title, detail, icon: null, card, run: () => onOpenTitle(card) });
      }
    }
    return out;
  });

  $effect(() => {
    void active;
    tick().then(() => list?.querySelector<HTMLElement>("[aria-selected='true']")?.scrollIntoView({ block: "nearest" }));
  });

  function onInput() {
    active = 0;
    clearTimeout(timer);
    const wanted = query.trim();
    if (wanted.length < 2) {
      titles = [];
      answered = wanted;
      return;
    }
    timer = setTimeout(() => void lookUp(wanted), 180);
  }

  async function lookUp(wanted: string) {
    const mine = ++request;
    try {
      const found = await api.search(wanted, SUGGESTIONS);
      if (mine !== request) return;
      titles = found;
      answered = wanted;
    } catch (e) {
      if (api.isSignedOut(e)) onSignedOut();
      else if (mine === request) {
        titles = [];
        answered = wanted;
      }
    }
  }

  function choose(row: Row | undefined) {
    if (!row) return;
    onClose();
    row.run();
  }

  // Captured, so these keys work here and nowhere else while it's open (Esc doesn't also leave
  // full screen). Everything else is typing.
  function onKey(e: KeyboardEvent) {
    switch (e.key) {
      case "Escape":
        onClose();
        break;
      case "ArrowDown":
        active = rows.length ? (active + 1) % rows.length : 0;
        break;
      case "ArrowUp":
        active = rows.length ? (active - 1 + rows.length) % rows.length : 0;
        break;
      case "Enter":
        choose(rows[active]);
        break;
      case "Tab":
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
  }
</script>

<svelte:window onkeydowncapture={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="overlay"
  onclick={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Jump to">
    <div class="field">
      <Icon name="search" size={16} />
      <input
        bind:this={input}
        bind:value={query}
        oninput={onInput}
        type="text"
        placeholder="Jump to a title, library, setting or page"
        autocomplete="off"
        spellcheck="false"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-results"
        aria-activedescendant={rows[active] ? `palette-${rows[active].key}` : undefined}
      />
      <kbd>Esc</kbd>
    </div>

    <ul class="results" id="palette-results" role="listbox" aria-label="Results" aria-busy={pending} bind:this={list}>
      {#each rows as row, i (row.key)}
        {#if i === 0 || rows[i - 1].group !== row.group}
          <li class="group" role="presentation">{row.group}</li>
        {/if}
        <!-- svelte-ignore a11y_click_events_have_key_events: the palette handles keys -->
        <li
          id="palette-{row.key}"
          role="option"
          aria-selected={i === active}
          class:is-active={i === active}
          onpointermove={() => (active = i)}
          onmousedown={(e) => e.preventDefault()}
          onclick={() => choose(row)}
        >
          {#if row.card}
            <span class="thumb"><Art image={row.card.image} title={row.card.title} width={40} /></span>
          {:else if row.icon}
            <span class="glyph"><Icon name={row.icon} size={16} /></span>
          {/if}
          <span class="text">
            <span class="name">{row.label}</span>
            {#if row.detail}<span class="meta">{row.detail}</span>{/if}
          </span>
        </li>
      {/each}
      {#if typed.length >= 2 && pending && !titles.length}
        <li class="note" role="presentation">Searching titles</li>
      {:else if typed.length >= 2 && !titles.length && !rows.length}
        <li class="note" role="presentation">Nothing called “{query.trim()}”</li>
      {:else if typed.length === 1}
        <li class="note" role="presentation">Keep typing to search titles</li>
      {/if}
    </ul>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    align-items: start;
    justify-items: center;
    padding: 12vh 20px 20px;
    background: color-mix(in srgb, var(--md-sys-color-scrim) 32%, transparent);
    animation: fade var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard) both;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-12px) scale(0.96);
    }
  }
  .palette {
    width: min(620px, 100%);
    overflow: hidden;
    color: var(--ink);
    animation: rise var(--md-sys-motion-duration-long) var(--md-sys-motion-emphasized-decelerate) both;
    background: var(--md-sys-color-surface-container-high);
    border-radius: var(--md-sys-shape-xl);
    box-shadow: var(--md-sys-elevation-3);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    height: 64px;
    padding: 0 20px;
    color: var(--md-sys-color-on-surface-variant);
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }
  .field input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    color: var(--ink);
    font: 400 16px/24px var(--f-ui);
    letter-spacing: 0.5px;
  }
  .field input::placeholder {
    color: var(--ink-3);
  }
  kbd {
    padding: 2px 7px;
    border: 1px solid var(--line);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--surface);
    color: var(--ink-2);
    font-family: inherit;
    font-size: 11.5px;
  }
  .results {
    max-height: min(58vh, 520px);
    overflow-y: auto;
    margin: 0;
    padding: 6px;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 8px 12px;
    border-radius: var(--md-sys-shape-lg);
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  li.is-active {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 10%, transparent);
    color: var(--md-sys-color-on-surface);
  }
  li.group {
    padding: 10px 8px 4px;
    font: 500 12px/16px var(--f-ui);
    letter-spacing: 0.5px;
    color: var(--md-sys-color-primary);
    cursor: default;
  }
  li.group:first-child {
    padding-top: 4px;
  }
  .glyph {
    display: grid;
    place-items: center;
    flex: none;
    width: 34px;
    height: 26px;
    color: var(--ink-3);
  }
  li.is-active .glyph {
    color: var(--accent);
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
  }
  .note {
    padding: 10px 8px;
    font-size: 13px;
    color: var(--ink-3);
    cursor: default;
  }
</style>
