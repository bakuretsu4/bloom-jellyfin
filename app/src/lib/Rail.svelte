<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import { reducedMotion } from "./settings.svelte";

  let { title, children }: { title: string; children: Snippet } = $props();

  let scroller = $state<HTMLDivElement>();
  let atStart = $state(true);
  let atEnd = $state(true);

  function measure() {
    if (!scroller) return;
    atStart = scroller.scrollLeft <= 2;
    atEnd = scroller.scrollLeft + scroller.clientWidth >= scroller.scrollWidth - 2;
  }

  $effect(() => {
    if (!scroller) return;
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(scroller);
    return () => ro.disconnect();
  });

  function page(direction: 1 | -1) {
    if (!scroller) return;
    const reduce = reducedMotion();
    scroller.scrollBy({ left: direction * scroller.clientWidth * 0.85, behavior: reduce ? "auto" : "smooth" });
  }
</script>

<section class="rail">
  <h2>{title}</h2>
  <div class="rail-wrap">
    <button class="rail-nav prev" aria-label={`Scroll ${title} back`} disabled={atStart} onclick={() => page(-1)}>
      <i><Icon name="chevl" size={16} /></i>
    </button>
    <div class="scroller" bind:this={scroller} onscroll={measure}>
      {@render children()}
    </div>
    <button class="rail-nav next" aria-label={`Scroll ${title} forward`} disabled={atEnd} onclick={() => page(1)}>
      <i><Icon name="chevr" size={16} /></i>
    </button>
  </div>
</section>

<style>
  h2 {
    margin: 0 0 12px;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.005em;
  }
  .rail-wrap {
    position: relative;
  }
  .scroller {
    display: flex;
    gap: 20px;
    overflow-x: auto;
    padding-bottom: 6px;
    scroll-snap-type: x proximity;
    scrollbar-width: none;
  }
  .scroller::-webkit-scrollbar {
    display: none;
  }
  .scroller > :global(*) {
    scroll-snap-align: start;
  }
  .rail-nav {
    position: absolute;
    top: 0;
    bottom: 26px;
    width: 62px;
    z-index: 3;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    cursor: pointer;
    color: var(--ink);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.18s var(--ease);
  }
  .rail-wrap:hover .rail-nav:not([disabled]),
  .rail-nav:focus-visible {
    opacity: 1;
    pointer-events: auto;
  }
  .rail-nav[disabled] {
    opacity: 0 !important;
    pointer-events: none;
  }
  .rail-nav.prev {
    left: -8px;
    justify-items: start;
    padding-left: 2px;
    background: linear-gradient(to right, var(--ground) 46%, transparent);
  }
  .rail-nav.next {
    right: -8px;
    justify-items: end;
    padding-right: 2px;
    background: linear-gradient(to left, var(--ground) 46%, transparent);
  }
  .rail-nav i {
    width: 34px;
    height: 34px;
    border-radius: var(--r-ctl);
    display: grid;
    place-items: center;
    background: var(--raise);
    border: 1px solid var(--line);
    box-shadow: var(--shadow);
    transition: background 0.16s var(--ease);
  }
  .rail-nav:hover i {
    background: var(--surface-2);
  }
</style>
