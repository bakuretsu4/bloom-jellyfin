<script lang="ts">
  import { tick } from "svelte";

  let { onClose }: { onClose: () => void } = $props();

  // Every shortcut that exists, where it works. Keep in step with the key handlers in App.svelte,
  // Watch.svelte, SearchBox.svelte and zoom.svelte.ts.
  const GROUPS: { title: string; rows: { label: string; keys: string[] }[] }[] = [
    {
      title: "Watching",
      rows: [
        { label: "Play or pause", keys: ["Space", "K"] },
        { label: "Back or forward 10 seconds", keys: ["← →", "J L"] },
        { label: "Volume", keys: ["↑ ↓"] },
        { label: "Mute", keys: ["M"] },
        { label: "Subtitles on or off", keys: ["C"] },
        { label: "Previous or next chapter", keys: ["[ ]"] },
        { label: "Skip intro or credits", keys: ["S"] },
        { label: "Next episode", keys: ["Shift N"] },
        { label: "Slower or faster", keys: ["<", ">"] },
        { label: "Jump to 10% to 90%", keys: ["1 – 9"] },
        { label: "Back to the start", keys: ["0", "Home"] },
        { label: "Theater mode", keys: ["T"] },
        { label: "Full screen", keys: ["F"] },
        { label: "Leave theater or full screen", keys: ["Esc"] },
      ],
    },
    {
      title: "Everywhere",
      rows: [
        { label: "Jump to a title, library, setting or page", keys: ["Ctrl K"] },
        { label: "Search", keys: ["/"] },
        { label: "Back", keys: ["Alt ←"] },
        { label: "Zoom in or out", keys: ["Ctrl +", "Ctrl −"] },
        { label: "Reset zoom", keys: ["Ctrl 0"] },
        { label: "This panel", keys: ["?"] },
      ],
    },
  ];

  let closeButton = $state<HTMLButtonElement>();
  const opener = document.activeElement as HTMLElement | null;

  $effect(() => {
    tick().then(() => closeButton?.focus());
    return () => opener?.focus?.();
  });

  // Modal: while open, keys go nowhere else, so Esc closes this rather than leaving full screen.
  function onKey(e: KeyboardEvent) {
    e.stopImmediatePropagation();
    if (e.key === "Escape" || e.key === "?") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Tab") {
      e.preventDefault();
      closeButton?.focus();
    }
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
  <div class="panel" role="dialog" aria-modal="true" aria-labelledby="shortcuts-title">
    <div class="head">
      <h2 id="shortcuts-title">Keyboard shortcuts</h2>
      <button class="btn" bind:this={closeButton} onclick={onClose}>Close</button>
    </div>
    {#each GROUPS as group (group.title)}
      <h3>{group.title}</h3>
      <div class="klist">
        {#each group.rows as row (row.label)}
          <div class="krow">
            <span>{row.label}</span>
            <span class="keys">
              {#each row.keys as key, i (key)}
                {#if i > 0}<span class="or">or</span>{/if}<kbd>{key}</kbd>
              {/each}
            </span>
          </div>
        {/each}
      </div>
    {/each}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 20px;
    background: color-mix(in srgb, var(--md-sys-color-scrim) 32%, transparent);
    animation: fade 0.2s var(--ease) both;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .panel {
    width: min(640px, 100%);
    max-height: 84dvh;
    overflow-y: auto;
    padding: 20px 22px 22px;
    color: var(--ink);
    background: var(--md-sys-color-surface-container-high);
    border-radius: var(--md-sys-shape-xl);
    box-shadow: var(--md-sys-elevation-3);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  h3 {
    margin: 18px 0 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink-2);
  }
  .klist {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 4px 24px;
  }
  .krow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-block: 4px;
    font-size: 13px;
    color: var(--ink-2);
  }
  .keys {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .or {
    font-size: 11.5px;
    color: var(--ink-3);
  }
  kbd {
    padding: 2px 7px;
    border: 1px solid var(--line);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--surface);
    color: var(--ink);
    font-family: inherit;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
