<script lang="ts">
  import * as api from "./api";
  import Icon from "./Icon.svelte";
  import { downloadFraction, downloadOf } from "./downloads.svelte";

  let {
    itemId,
    iconSize = 16,
    onOpenDownloads,
    onError,
  }: {
    itemId: string;
    iconSize?: number;
    /** Once it's listed, the button leads to the Downloads screen rather than acting again. */
    onOpenDownloads: () => void;
    onError: (e: unknown) => void;
  } = $props();

  let busy = $state(false);
  let download = $derived(downloadOf(itemId));
  let label = $derived.by(() => {
    switch (download?.status) {
      case undefined:
        return "Download";
      case "done":
        return "Downloaded";
      case "downloading":
        return `Downloading ${Math.round(downloadFraction(download) * 100)}%`;
      case "queued":
        return "Queued";
      case "paused":
        return "Download paused";
      case "failed":
        return "Download failed";
    }
  });

  async function click() {
    if (download) {
      onOpenDownloads();
      return;
    }
    busy = true;
    try {
      await api.downloadItem(itemId);
    } catch (e) {
      onError(e);
    } finally {
      busy = false;
    }
  }
</script>

<button class="btn" aria-pressed={download?.status === "done"} disabled={busy} onclick={click}>
  <Icon name={download?.status === "done" ? "check" : "download"} size={iconSize} />{label}
</button>
