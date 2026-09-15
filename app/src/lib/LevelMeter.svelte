<script lang="ts">
  // No props: the meter only ever means "waiting".
</script>

<!-- Buffering (DESIGN.md §7): a five-bar level meter. Each bar keeps its own period, so it reads
     as a meter rather than a wave. -->
<span class="meter" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>

<style>
  .meter {
    /* content-box: the global border-box rule would let the padding eat the bars' height */
    box-sizing: content-box;
    display: flex;
    align-items: flex-end;
    gap: 5px;
    height: 30px;
    padding: 14px 16px;
    border-radius: var(--r-ctl);
    background: rgba(18, 20, 19, 0.74);
  }
  .meter i {
    width: 6px;
    height: 100%;
    background: var(--accent-media);
    transform-origin: bottom;
    animation: level 0.9s var(--ease-mech) infinite;
  }
  .meter i:nth-child(1) {
    animation-duration: 0.78s;
    animation-delay: -0.3s;
  }
  .meter i:nth-child(2) {
    animation-duration: 0.94s;
    animation-delay: -0.62s;
  }
  .meter i:nth-child(3) {
    animation-duration: 0.86s;
    animation-delay: -0.12s;
  }
  .meter i:nth-child(4) {
    animation-duration: 1.02s;
    animation-delay: -0.48s;
  }
  .meter i:nth-child(5) {
    animation-duration: 0.9s;
    animation-delay: -0.74s;
  }
  @keyframes level {
    0%,
    100% {
      transform: scaleY(0.28);
    }
    30% {
      transform: scaleY(1);
    }
    62% {
      transform: scaleY(0.52);
    }
  }
</style>
