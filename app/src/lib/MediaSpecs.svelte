<script lang="ts" module>
  function size(bytes: number): string {
    return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`;
  }
</script>

<script lang="ts">
  import type { MediaInfo } from "./api";

  let { media, studios = [] }: { media: MediaInfo; studios?: string[] } = $props();
</script>

<!-- The file behind an item, on its page and under the player. -->
<dl class="specs">
  {#if media.container}<div class="spec"><dt>Container</dt><dd>{media.container}</dd></div>{/if}
  {#if media.video.length}<div class="spec"><dt>Video</dt><dd>{media.video.join(", ")}</dd></div>{/if}
  {#if media.audio.length}<div class="spec"><dt>Audio</dt><dd>{media.audio.join(", ")}</dd></div>{/if}
  {#if media.subtitles.length}<div class="spec"><dt>Subtitles</dt><dd>{media.subtitles.join(", ")}</dd></div>{/if}
  {#if media.sizeBytes}<div class="spec"><dt>Size</dt><dd>{size(media.sizeBytes)}</dd></div>{/if}
  {#if media.bitrate}<div class="spec"><dt>Bitrate</dt><dd>{(media.bitrate / 1e6).toFixed(1)} Mb/s</dd></div>{/if}
  {#if studios.length}<div class="spec"><dt>Studio</dt><dd>{studios.join(", ")}</dd></div>{/if}
</dl>

<style>
  .specs {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 14px 20px;
    margin: 0;
  }
  .spec dt {
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .spec dd {
    margin: 0;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
</style>
