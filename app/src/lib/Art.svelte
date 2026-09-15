<script lang="ts" module>
  // Muted, similar-lightness tints so title cards read as a set, not as a colour-coded system.
  const TINTS = ["#5A6B78", "#63705C", "#54636B", "#5E6A74", "#6E6A64", "#6B5F66", "#5F6F6A"];

  function tintFor(title: string): string {
    let h = 7;
    for (let i = 0; i < title.length; i++) h = (h * 31 + title.charCodeAt(i)) >>> 0;
    return TINTS[h % TINTS.length];
  }
</script>

<script lang="ts">
  import { imageUrl, type Image } from "./api";

  let {
    image,
    title,
    sub = null,
    width,
  }: { image: Image | null; title: string; sub?: string | null; width: number } = $props();

  let failed = $state(false);
  let loaded = $state(false);
  let src = $derived(image && !failed ? imageUrl(image, width) : null);
</script>

<!-- The title card is the fallback for missing or broken artwork. While an image is still on
     its way the block stays plain, so cached artwork doesn't flash a label first. -->
<span class="art" style:--tint={tintFor(title)}>
  {#if !src}
    <span class="art-label"><b>{title}</b>{#if sub}<span>{sub}</span>{/if}</span>
  {:else}
    <img
      {src}
      alt=""
      loading="lazy"
      decoding="async"
      class:is-loaded={loaded}
      onload={() => (loaded = true)}
      onerror={() => (failed = true)}
    />
  {/if}
</span>
