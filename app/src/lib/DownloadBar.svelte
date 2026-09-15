<script lang="ts" module>
  // Fixed rather than random, so the bar looks the same every time and never clumps.
  const BUBBLES = [
    { x: 8, size: 4, dur: 2.1, delay: -0.2 },
    { x: 21, size: 6, dur: 2.6, delay: -1.4 },
    { x: 34, size: 3, dur: 1.8, delay: -0.9 },
    { x: 47, size: 5, dur: 2.4, delay: -2.0 },
    { x: 61, size: 4, dur: 1.9, delay: -0.5 },
    { x: 74, size: 6, dur: 2.7, delay: -1.8 },
    { x: 88, size: 3, dur: 2.0, delay: -1.1 },
  ];
</script>

<script lang="ts">
  let {
    progress,
    active = true,
    label,
  }: {
    /** 0..1. */
    progress: number;
    /** Transferring now. Paused or queued, it's a still, thinner bar. */
    active?: boolean;
    label: string;
  } = $props();

  let percent = $derived(Math.round(Math.min(Math.max(progress, 0), 1) * 100));
</script>

<!-- Downloading (DESIGN.md §7): the one bubbly thing in Bloom. Bubbles rise through the filled
     part and pop, and the leading edge swells gently, while a transfer is running. -->
<div
  class="bar"
  class:is-active={active}
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={percent}
>
  <span class="fill" style:width="{percent}%">
    {#if active && percent > 0}
      <span class="bubbles" aria-hidden="true">
        {#each BUBBLES as bubble, i (i)}
          <i
            style:left="{bubble.x}%"
            style:--size="{bubble.size}px"
            style:animation-duration="{bubble.dur}s"
            style:animation-delay="{bubble.delay}s"
          ></i>
        {/each}
      </span>
    {/if}
  </span>
</div>

<style>
  .bar {
    position: relative;
    height: 5px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--surface-2);
    transition: height 0.2s var(--ease);
  }
  .bar.is-active {
    height: 12px;
  }
  .fill {
    position: relative;
    display: block;
    height: 100%;
    border-radius: 999px;
    background: linear-gradient(180deg, color-mix(in oklab, var(--accent) 78%, #fff), var(--accent) 70%);
    transition: width 0.4s var(--ease);
  }
  /* A soft blob at the leading edge that swells and settles, like liquid finding its level. */
  .is-active .fill::after {
    content: "";
    position: absolute;
    right: -3px;
    top: 50%;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: color-mix(in oklab, var(--accent) 86%, #fff);
    transform: translateY(-50%);
    animation: swell 1.4s ease-in-out infinite;
  }
  .bubbles {
    position: absolute;
    inset: 0;
    overflow: hidden;
    border-radius: inherit;
  }
  .bubbles i {
    position: absolute;
    bottom: -2px;
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.75);
    background: rgba(255, 255, 255, 0.28);
    opacity: 0;
    animation: rise 2.2s ease-in infinite;
  }
  @keyframes rise {
    0% {
      transform: translate(0, 4px) scale(0.35);
      opacity: 0;
    }
    20% {
      opacity: 1;
    }
    60% {
      transform: translate(-3px, -4px) scale(0.9);
      opacity: 0.95;
    }
    85% {
      transform: translate(2px, -9px) scale(1.1);
      opacity: 0.8;
    }
    100% {
      transform: translate(0, -12px) scale(1.5);
      opacity: 0;
    }
  }
  @keyframes swell {
    0%,
    100% {
      transform: translateY(-50%) scale(0.85);
    }
    50% {
      transform: translateY(-50%) scale(1.15);
    }
  }
</style>
