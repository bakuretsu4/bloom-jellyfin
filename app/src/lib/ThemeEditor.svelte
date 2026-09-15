<script lang="ts">
  import * as api from "./api";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";
  import { checkTheme, normaliseHex, THEME_STARTS, themeFromCss, themeProperties, themeToCss } from "./theme";

  let {
    themes,
    activeId,
    onChange,
  }: {
    themes: api.CustomTheme[];
    activeId: string | null;
    /** The themes as they now stand, and the one in use (null for Bloom's own). */
    onChange: (themes: api.CustomTheme[], activeId: string | null) => void;
  } = $props();

  type ColourKey = "ground" | "surface" | "ink" | "accent";
  const COLOURS: { key: ColourKey; label: string }[] = [
    { key: "ground", label: "Background" },
    { key: "surface", label: "Panels" },
    { key: "ink", label: "Text" },
    { key: "accent", label: "Accent" },
  ];

  let draft = $state<api.CustomTheme | null>(null);
  let existing = $state(false);
  /** Editing as CSS: the text as typed, and what couldn't be used from it. */
  let asCss = $state(false);
  let cssText = $state("");
  let cssProblems = $state<string[]>([]);
  let copied = $state(false);
  let fonts = $state<string[] | null>(null);

  let checks = $derived(draft ? checkTheme(draft) : []);
  let usable = $derived(checks.every((check) => check.ok));
  let handSet = $derived(Object.keys(draft?.tokens ?? {}).length);
  let previewStyle = $derived(
    draft
      ? Object.entries(themeProperties(draft))
          .map(([property, value]) => `${property.replace(/^--/, "--p-")}: ${value}`)
          .join("; ")
      : "",
  );
  let fontOptions = $derived([
    { value: "", label: "Archivo (Bloom's)" },
    ...(fonts ?? []).filter((f) => f !== "Archivo").map((f) => ({ value: f, label: f })),
    // A font from pasted CSS that isn't installed here still shows as chosen.
    ...(draft?.font && fonts && !fonts.includes(draft.font) ? [{ value: draft.font, label: `${draft.font} (not installed)` }] : []),
  ]);

  $effect(() => {
    if (!draft || fonts) return;
    api
      .systemFonts()
      .then((found) => (fonts = found))
      .catch(() => (fonts = []));
  });

  function newId(): string {
    return typeof crypto.randomUUID === "function" ? crypto.randomUUID() : `t${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
  }

  function open(theme: api.CustomTheme, isExisting: boolean) {
    draft = { tokens: {}, font: null, ...theme };
    existing = isExisting;
    asCss = false;
    cssProblems = [];
  }

  /** Light and dark start from Bloom's own colours for that side. */
  function setBase(base: api.CustomTheme["base"]) {
    if (!draft || draft.base === base) return;
    draft = { ...draft, base, ...THEME_STARTS[base] };
  }

  function typeHex(key: ColourKey, value: string) {
    if (!draft) return;
    const hex = normaliseHex(value.trim().startsWith("#") ? value : `#${value}`);
    if (hex) draft[key] = hex;
  }

  function toggleCss() {
    if (!draft) return;
    asCss = !asCss;
    if (asCss) {
      cssText = themeToCss(draft);
      cssProblems = [];
    }
  }

  function editCss(text: string) {
    if (!draft) return;
    cssText = text;
    const { theme, problems } = themeFromCss(text, draft);
    cssProblems = problems;
    draft = theme;
  }

  async function copyCss() {
    if (!draft) return;
    try {
      await navigator.clipboard.writeText(themeToCss(draft));
      copied = true;
      setTimeout(() => (copied = false), 1600);
    } catch {
      // Nothing to copy to: the CSS is still there to select.
      asCss = true;
      cssText = themeToCss(draft);
    }
  }

  function save() {
    if (!draft || !usable) return;
    const saved = { ...draft, name: draft.name.trim() || "Custom theme" };
    const next = existing ? themes.map((t) => (t.id === saved.id ? saved : t)) : [...themes, saved];
    onChange(next, saved.id);
    draft = null;
  }

  function remove() {
    if (!draft) return;
    const id = draft.id;
    onChange(
      themes.filter((t) => t.id !== id),
      activeId === id ? null : activeId,
    );
    draft = null;
  }
</script>

<div class="custom">
  <div class="chips">
    {#each themes as theme (theme.id)}
      <div class="chip" class:is-active={theme.id === activeId}>
        <button class="chip-use" aria-pressed={theme.id === activeId} onclick={() => onChange(themes, theme.id)}>
          <span class="swatches" aria-hidden="true">
            {#each [theme.ground, theme.surface, theme.ink, theme.accent] as colour, i (i)}<i style:background={colour}></i>{/each}
          </span>
          {theme.name}
        </button>
        <button class="chip-edit" aria-label="Edit {theme.name}" onclick={() => open(theme, true)}><Icon name="gear" size={13} /></button>
      </div>
    {/each}
    {#if !draft}
      <button class="btn" onclick={() => open({ id: newId(), name: "My theme", base: "dark", ...THEME_STARTS.dark }, false)}>New theme</button>
    {/if}
  </div>

  {#if draft}
    <div class="editor">
      <div class="editor-fields">
        <label class="name">
          <span>Name</span>
          <input type="text" maxlength="40" spellcheck="false" bind:value={draft.name} />
        </label>

        <div class="mode">
          <div class="seg" role="group" aria-label="Light or dark">
            <button type="button" aria-pressed={draft.base === "dark"} onclick={() => setBase("dark")}>Dark</button>
            <button type="button" aria-pressed={draft.base === "light"} onclick={() => setBase("light")}>Light</button>
          </div>
          <button class="btn" aria-pressed={asCss} onclick={toggleCss}>{asCss ? "Back to colours" : "Edit as CSS"}</button>
        </div>

        {#if asCss}
          <label class="css">
            <span class="sr-only">The theme as CSS</span>
            <textarea spellcheck="false" rows="17" value={cssText} oninput={(e) => editCss(e.currentTarget.value)}></textarea>
          </label>
          {#if cssProblems.length}
            <ul class="problems" role="alert">
              {#each cssProblems as problem, i (i)}<li>{problem}</li>{/each}
            </ul>
          {:else}
            <p class="css-hint">Bloom's colour tokens as hex, <code>--f-ui</code> for the font and <code>color-scheme</code> for light or dark.</p>
          {/if}
        {:else}
          {#each COLOURS as colour (colour.key)}
            <div class="colour">
              <span>{colour.label}</span>
              <input type="color" aria-label="{colour.label} colour" bind:value={draft[colour.key]} />
              <input
                class="hex"
                type="text"
                spellcheck="false"
                maxlength="7"
                aria-label="{colour.label} as a hex code"
                value={draft[colour.key]}
                onchange={(e) => typeHex(colour.key, e.currentTarget.value)}
              />
            </div>
          {/each}
          {#if handSet}
            <p class="hand-set">
              {handSet} {handSet === 1 ? "shade" : "shades"} set by hand in CSS.
              <button class="text-button" onclick={() => draft && (draft.tokens = {})}>Clear</button>
            </p>
          {/if}
          <div class="colour">
            <span>Font</span>
            <div class="font">
              {#if fonts}
                <Select label="Font" value={draft.font ?? ""} options={fontOptions} onChange={(value) => draft && (draft.font = value || null)} />
              {:else}
                <span class="loading">Reading fonts</span>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <div class="editor-side">
        <!-- The theme's own properties on a copy of a panel, so it can be judged before it's used. -->
        <div class="preview" style={previewStyle} aria-label="Preview">
          <div class="preview-panel">
            <span class="preview-title">Tokyo Ghoul</span>
            <span class="preview-meta">2014, 2 seasons, 24 episodes</span>
            <span class="preview-faint">S1 E4, Dinner</span>
            <span class="preview-actions">
              <span class="preview-primary">Play</span>
              <span class="preview-link">More details</span>
            </span>
          </div>
        </div>

        <ul class="checks">
          {#each checks as check (check.label)}
            <li class:is-bad={!check.ok}>
              <Icon name={check.ok ? "check" : "close"} size={12} />
              {check.label}
              <span class="ratio">{check.ratio.toFixed(1)}:1{check.ok ? "" : `, needs ${check.needed}:1`}</span>
            </li>
          {/each}
        </ul>

        <div class="editor-actions">
          <button class="btn btn-primary" disabled={!usable} onclick={save}>Save and use</button>
          <button class="btn" onclick={copyCss}>{copied ? "Copied" : "Copy CSS"}</button>
          <button class="btn" onclick={() => (draft = null)}>Cancel</button>
          {#if existing}<button class="btn is-danger" onclick={remove}>Delete</button>{/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .custom {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--surface);
  }
  .chip.is-active {
    border-color: var(--accent);
  }
  .chip-use,
  .chip-edit {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px;
    border: 0;
    background: none;
    color: var(--ink-2);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .chip-use[aria-pressed="true"] {
    color: var(--ink);
  }
  .chip-edit {
    padding: 0 8px;
    border-left: 1px solid var(--line-soft);
    color: var(--ink-3);
  }
  .chip-use:hover,
  .chip-edit:hover {
    color: var(--ink);
  }
  .swatches {
    display: inline-flex;
    gap: 2px;
  }
  .swatches i {
    width: 10px;
    height: 16px;
    border-radius: 2px;
    box-shadow: inset 0 0 0 1px rgba(128, 128, 128, 0.35);
  }

  .editor {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 20px;
    padding: 16px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--surface);
  }
  .editor-fields,
  .editor-side {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12.5px;
    color: var(--ink-2);
  }
  input[type="text"],
  textarea {
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--ground);
    color: var(--ink);
    font: inherit;
    font-size: 13.5px;
  }
  input[type="text"] {
    height: 32px;
  }
  input[type="text"]:focus-visible,
  textarea:focus-visible {
    outline: none;
    border-color: var(--accent);
  }
  .mode {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .mode .btn[aria-pressed="true"] {
    color: var(--accent);
    border-color: color-mix(in oklab, var(--accent) 42%, var(--line));
  }
  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--ground);
  }
  .seg button {
    padding: 4px 12px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--ink-2);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .seg button[aria-pressed="true"] {
    background: var(--raise);
    color: var(--ink);
  }
  .colour {
    display: grid;
    grid-template-columns: 90px 40px minmax(0, 96px);
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--ink-2);
  }
  .colour .font {
    grid-column: 2 / -1;
    min-width: 0;
  }
  .loading {
    font-size: 12.5px;
    color: var(--ink-3);
  }
  input[type="color"] {
    width: 40px;
    height: 30px;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--ground);
    cursor: pointer;
  }
  .hex {
    font-variant-numeric: tabular-nums;
  }
  .hand-set {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-3);
  }
  .text-button {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
  .css {
    display: block;
  }
  textarea {
    width: 100%;
    padding: 10px 12px;
    resize: vertical;
    font-family: ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", monospace;
    font-size: 12.5px;
    line-height: 1.55;
    tab-size: 2;
  }
  .css-hint,
  .problems {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-3);
  }
  .problems {
    padding-left: 18px;
    color: var(--alert);
  }
  code {
    font-family: ui-monospace, monospace;
    font-size: 12px;
    color: var(--ink-2);
  }

  .preview {
    padding: 14px;
    border-radius: var(--r);
    background: var(--p-ground);
    border: 1px solid var(--p-line);
    font-family: var(--p-f-ui, var(--f-ui));
  }
  .preview-panel {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 12px 14px;
    border-radius: var(--r);
    background: var(--p-surface);
    border: 1px solid var(--p-line-soft);
  }
  .preview-title {
    font-weight: 600;
    font-size: 15px;
    color: var(--p-ink);
  }
  .preview-meta {
    font-size: 12.5px;
    color: var(--p-ink-2);
  }
  .preview-faint {
    font-size: 12.5px;
    color: var(--p-ink-3);
  }
  .preview-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 8px;
  }
  .preview-primary {
    padding: 4px 12px;
    border-radius: var(--r-ctl);
    background: var(--p-accent);
    color: var(--p-accent-ink);
    font-size: 13px;
    font-weight: 500;
  }
  .preview-link {
    font-size: 13px;
    color: var(--p-accent);
  }

  .checks {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12.5px;
    color: var(--ink-2);
  }
  .checks li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .checks li :global(svg) {
    color: var(--good);
  }
  .checks li.is-bad,
  .checks li.is-bad :global(svg) {
    color: var(--alert);
  }
  .ratio {
    margin-left: auto;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .checks li.is-bad .ratio {
    color: var(--alert);
  }
  .editor-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .btn.is-danger {
    color: var(--alert);
    border-color: color-mix(in oklab, var(--alert) 45%, var(--line));
  }

  @media (max-width: 760px) {
    .editor {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
