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
    gap: 5px;
    padding: 20px 0 6px;
  }
  button {
    min-width: 34px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--ink-3);
    font-size: 13.5px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    transition: background 0.16s var(--ease), color 0.16s var(--ease);
  }
  button:hover:not([disabled]) {
    background: var(--surface-2);
    color: var(--ink);
  }
  button[aria-current="page"] {
    background: var(--accent);
    color: var(--accent-ink);
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
