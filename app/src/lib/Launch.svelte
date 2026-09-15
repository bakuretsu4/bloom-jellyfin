<script lang="ts">
  import { untrack } from "svelte";
  import Mark from "./Mark.svelte";

  let {
    onCovered,
    onDone,
  }: {
    /** The roll hides the window now: swap the screen underneath, unseen. */
    onCovered: () => void;
    onDone: () => void;
  } = $props();

  let done = $state(false);

  // Timed to the roll: covered almost at once, locked by 1.1s, faded out by 1.7s.
  $effect(() => {
    const timers = untrack(() => [
      setTimeout(onCovered, 240),
      setTimeout(() => (done = true), 1360),
      setTimeout(onDone, 1700),
    ]);
    return () => timers.forEach(clearTimeout);
  });
</script>

<!-- Signal acquire (DESIGN.md §7): the picture rolls like an unsynced monitor, slows, and locks.
     Always dark, in both themes: it's one moment, not a surface. -->
<div class="launch" class:is-done={done} aria-hidden="true">
  <div class="frame">
    <div class="roll">
      <div class="pic">
        <span class="mark"><Mark size={64} /></span>
        <span class="word">Bloom</span>
      </div>
      <div class="bar"></div>
      <div class="pic is-above">
        <span class="mark"><Mark size={64} /></span>
        <span class="word">Bloom</span>
      </div>
    </div>
  </div>
</div>

<style>
  .launch {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding-inline: 16px;
    overflow: hidden;
    background: #0e100f;
    transition: opacity 0.32s var(--ease-mech);
  }
  .launch.is-done {
    opacity: 0;
    pointer-events: none;
  }
  .frame {
    position: relative;
    width: min(520px, 100%);
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border-radius: var(--r-lg);
    background: #141615;
  }
  .roll {
    position: absolute;
    inset: 0;
    will-change: transform;
    animation: roll 1.1s both;
  }
  .pic {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 16px;
  }
  .pic.is-above {
    top: -114%;
  }
  /* The blanking interval between two frames of the rolling picture. */
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    top: -14%;
    height: 14%;
    background: #0a0b0b;
  }
  .mark {
    display: grid;
    color: #d98237;
  }
  .word {
    font-stretch: 118%;
    font-weight: 600;
    letter-spacing: -0.015em;
    font-size: clamp(28px, 6vw, 44px);
    color: #e9eceb;
  }
  /* Three rolls, each slower than the last, then a small jolt as it locks. */
  @keyframes roll {
    0% {
      transform: translateY(0);
      animation-timing-function: linear;
    }
    18% {
      transform: translateY(114%);
    }
    18.01% {
      transform: translateY(0);
      animation-timing-function: cubic-bezier(0.3, 0.1, 0.6, 1);
    }
    46% {
      transform: translateY(114%);
    }
    46.01% {
      transform: translateY(0);
      animation-timing-function: cubic-bezier(0.16, 0.84, 0.24, 1);
    }
    86% {
      transform: translateY(114%);
    }
    90% {
      transform: translateY(116.5%);
    }
    93% {
      transform: translateY(113%);
    }
    100% {
      transform: translateY(114%);
    }
  }
</style>
