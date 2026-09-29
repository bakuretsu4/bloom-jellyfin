<script lang="ts">
  let {
    page,
    pages,
    onPage,
    label = "Pages",
  }: { page: number; pages: number; onPage: (page: number) => void; label?: string } = $props();

  // First, last, and the pages either side of the current one; a gap stands for the rest.
  let items = $derived.by(() => {
    const out: (number | "gap")[] = [];
    for (let i = 0; i < pages; i++) {
      if (i === 0 || i === pages - 1 || Math.abs(i - page) <= 1) out.push(i);
      else if (out[out.length - 1] !== "gap") out.push("gap");
    }
    return out;
  });
</script>

{#if pages > 1}
  <nav class="pager" aria-label={label}>
    <button type="button" disabled={page === 0} onclick={() => onPage(page - 1)}>Previous</button>
    {#each items as item, i (i)}
      {#if item === "gap"}
        <span class="gap" aria-hidden="true">…</span>
      {:else}
        <button
          type="button"
          aria-current={item === page ? "page" : undefined}
          aria-label={`Page ${item + 1}`}
          onclick={() => onPage(item)}>{item + 1}</button
        >
      {/if}
    {/each}
    <button type="button" disabled={page === pages - 1} onclick={() => onPage(page + 1)}>Next</button>
  </nav>
{/if}

<style>
  .pager {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 24px 0 8px;
  }
  button {
    min-width: 40px;
    height: 40px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--md-sys-shape-full);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    font: 500 14px/20px var(--f-ui);
    letter-spacing: 0.1px;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    transition:
      background-color var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard),
      color var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard);
  }
  button:hover:not([disabled]) {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    color: var(--md-sys-color-on-surface);
  }
  button[aria-current="page"] {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  button[disabled] {
    opacity: 0.35;
    cursor: default;
  }
  .gap {
    padding: 0 2px;
    color: var(--ink-3);
  }
</style>
