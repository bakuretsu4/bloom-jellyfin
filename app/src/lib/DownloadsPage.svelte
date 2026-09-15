<script lang="ts" module>
  import type { Download, DownloadStatus, Quality } from "./api";

  const QUALITY_OPTIONS: { value: Quality; label: string }[] = [
    { value: "original", label: "Original" },
    { value: "1080p", label: "1080p" },
    { value: "720p", label: "720p" },
  ];
  const ORDER: Record<DownloadStatus, number> = { downloading: 0, queued: 1, paused: 2, failed: 3, done: 4 };

  function duration(seconds: number): string {
    const minutes = Math.max(1, Math.round(seconds / 60));
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return h ? (m ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
  }

  const byStatus = (a: Download, b: Download) => ORDER[a.status] - ORDER[b.status] || a.addedAt - b.addedAt;
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "./api";
  import Art from "./Art.svelte";
  import DownloadBar from "./DownloadBar.svelte";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";
  import { bytesLabel, downloadFraction, downloads } from "./downloads.svelte";
  import { markSource } from "./motion";

  let {
    serverName,
    offline,
    onPlay,
  }: { serverName: string; offline: boolean; onPlay: (itemId: string) => void } = $props();

  let storage = $state<api.DownloadStorage | null>(null);
  /** A finished download whose delete button has been pressed once. */
  let confirming = $state<string | null>(null);
  let problem = $state("");
  const arts: Record<string, HTMLElement> = {};

  let inProgress = $derived(downloads.list.filter((d) => d.status !== "done").sort(byStatus));
  let ready = $derived(
    downloads.list.filter((d) => d.status === "done").sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0)),
  );

  // The disk readout is read again whenever a download starts, finishes or goes.
  let listShape = $derived(downloads.list.map((d) => `${d.id}:${d.status}`).join());
  $effect(() => {
    void listShape;
    const first = untrack(() => storage) === null;
    const timer = setTimeout(
      () =>
        api
          .downloadStorage()
          .then((loaded) => (storage = loaded))
          .catch(() => {}),
      first ? 0 : 800,
    );
    return () => clearTimeout(timer);
  });

  let capacity = $derived.by(() => {
    if (!storage?.totalBytes || storage.freeBytes == null) return null;
    const total = storage.totalBytes;
    const bloom = Math.min(storage.bloomBytes, total);
    const other = Math.max(0, total - storage.freeBytes - bloom);
    return { bloom, other, free: storage.freeBytes, total };
  });

  $effect(() => {
    if (!confirming) return;
    const timer = setTimeout(() => (confirming = null), 4000);
    return () => clearTimeout(timer);
  });

  function sizeOf(d: Download): string {
    const about = d.estimated ? "about " : "";
    return d.bytesTotal ? `${bytesLabel(d.bytesDone)} of ${about}${bytesLabel(d.bytesTotal)}` : bytesLabel(d.bytesDone);
  }

  function metaOf(d: Download): string[] {
    switch (d.status) {
      case "downloading": {
        const parts = [sizeOf(d)];
        if (!d.bytesPerSecond) parts.push("Starting");
        else {
          parts.push(`${bytesLabel(d.bytesPerSecond)}/s`);
          if (d.bytesTotal && !d.estimated) {
            const left = (d.bytesTotal - d.bytesDone) / d.bytesPerSecond;
            parts.push(left < 60 ? "Under a minute left" : `${duration(left)} left`);
          }
        }
        return parts;
      }
      case "queued": {
        const state =
          d.waiting === "network" ? "Waits for the local network" : d.waiting === "server" ? `Waiting for ${serverName}` : "Queued";
        return d.bytesTotal ? [state, `${d.estimated ? "about " : ""}${bytesLabel(d.bytesTotal)}`] : [state];
      }
      case "paused":
        return d.quality === "original" ? ["Paused", sizeOf(d)] : ["Paused", "Starts again when resumed"];
      case "failed":
        return [d.bytesDone ? sizeOf(d) : "Not started"];
      case "done": {
        const parts = [bytesLabel(d.bytesDone)];
        if (d.quality !== "original") parts.push(d.quality);
        if (d.played) parts.push("Watched");
        else if (d.positionSeconds > 0 && d.runtimeSeconds) parts.push(`${duration(d.runtimeSeconds - d.positionSeconds)} left`);
        else if (d.runtimeSeconds) parts.push(duration(d.runtimeSeconds));
        return parts;
      }
    }
  }

  async function act(call: Promise<unknown>) {
    problem = "";
    try {
      await call;
    } catch (e) {
      problem = api.message(e);
    }
  }

  /** A finished download asks once more before its file is deleted. */
  function remove(d: Download) {
    if (d.status === "done" && confirming !== d.id) {
      confirming = d.id;
      return;
    }
    confirming = null;
    void act(api.removeDownload(d.id));
  }

  function play(d: Download) {
    markSource(arts[d.id]);
    onPlay(d.itemId);
  }
</script>

{#snippet row(d: Download)}
  {@const done = d.status === "done"}
  {@const name = d.subtitle ? `${d.title}, ${d.subtitle}` : d.title}
  <div class="dl">
    {#if done}
      <button class="dl-art" bind:this={arts[d.id]} aria-label="Play {name}" onclick={() => play(d)}>
        <Art image={d.image} title={d.title} width={160} />
        <span class="dl-play" aria-hidden="true"><Icon name="play" size={18} /></span>
        {#if !d.played && d.positionSeconds > 0 && d.runtimeSeconds}
          <span class="dl-progress"><span style:width="{Math.min(100, (d.positionSeconds / d.runtimeSeconds) * 100)}%"></span></span>
        {/if}
      </button>
    {:else}
      <span class="dl-art"><Art image={d.image} title={d.title} width={160} /></span>
    {/if}

    <div class="dl-body">
      <span class="dl-title">{d.title}</span>
      {#if d.subtitle}<span class="dl-sub">{d.subtitle}</span>{/if}
      <span class="dl-meta">{#each metaOf(d) as part, i (i)}<span>{part}</span>{/each}</span>
      {#if d.status === "failed" && d.error}<span class="dl-error">{d.error}</span>{/if}
      {#if !done}
        <span class="dl-bar">
          <DownloadBar progress={downloadFraction(d)} active={d.status === "downloading"} label="{name}, downloaded" />
        </span>
      {/if}
    </div>

    <div class="dl-actions">
      {#if !done && d.qualities.length > 1}
        <Select
          label="Quality for {name}"
          value={d.quality}
          options={QUALITY_OPTIONS.filter((option) => d.qualities.includes(option.value))}
          onChange={(quality) => act(api.setDownloadQuality(d.id, quality))}
        />
      {/if}
      {#if d.status === "downloading" || d.status === "queued"}
        <button class="iconbtn" aria-label="Pause {name}" onclick={() => act(api.pauseDownload(d.id))}>
          <Icon name="pause" size={16} />
        </button>
      {:else if d.status === "paused"}
        <button class="iconbtn" aria-label="Resume {name}" onclick={() => act(api.resumeDownload(d.id))}>
          <Icon name="play" size={16} />
        </button>
      {:else if d.status === "failed"}
        <button class="btn" onclick={() => act(api.resumeDownload(d.id))}>Try again</button>
      {/if}
      {#if !done}
        <button class="iconbtn" aria-label="Cancel {name}" onclick={() => remove(d)}><Icon name="close" size={16} /></button>
      {:else if confirming === d.id}
        <button class="btn is-danger" onclick={() => remove(d)}>Delete {bytesLabel(d.bytesDone)}</button>
      {:else}
        <button class="iconbtn" aria-label="Delete {name}" onclick={() => remove(d)}><Icon name="trash" size={16} /></button>
      {/if}
    </div>
  </div>
{/snippet}

<div class="page">
  <header class="page-head">
    <div class="page-heading">
      <h1 class="page-title">Downloads</h1>
      <span class="eyebrow">{storage ? `Saved in ${storage.location}` : "Saved on this computer"}</span>
    </div>
  </header>

  {#if offline}
    <p class="offline-note" role="status">
      {serverName} isn't answering. Everything downloaded still plays, and where you stop watching is sent to it once
      it's back.
    </p>
  {/if}

  {#if capacity}
    <div class="storage">
      <div class="capacity" aria-hidden="true">
        <span class="is-bloom" style:width="{(capacity.bloom / capacity.total) * 100}%"></span>
        <span class="is-other" style:width="{(capacity.other / capacity.total) * 100}%"></span>
      </div>
      <div class="legend">
        <span><i class="is-bloom"></i>Bloom {bytesLabel(capacity.bloom)}</span>
        <span><i class="is-other"></i>Other files {bytesLabel(capacity.other)}</span>
        <span><i class="is-free"></i>Free {bytesLabel(capacity.free)}</span>
      </div>
    </div>
  {/if}

  {#if problem}<p class="problem" role="alert">{problem}</p>{/if}

  {#if downloads.list.length === 0}
    <div class="empty">
      <div class="empty-art" aria-hidden="true"><span></span><span></span><span></span></div>
      <h2>Nothing downloaded yet</h2>
      <p>Download a film or an episode from its page, or a whole season at once, to watch it without a connection.</p>
    </div>
  {:else}
    {#if inProgress.length}
      <section class="group" aria-labelledby="dl-progress">
        <h2 class="group-title" id="dl-progress">In progress</h2>
        {#each inProgress as d (d.id)}{@render row(d)}{/each}
      </section>
    {/if}
    {#if ready.length}
      <section class="group" aria-labelledby="dl-ready">
        <h2 class="group-title" id="dl-ready">Ready to watch<span class="eyebrow">{ready.length}</span></h2>
        {#each ready as d (d.id)}{@render row(d)}{/each}
      </section>
    {/if}
  {/if}
</div>

<style>
  .offline-note {
    max-width: 70ch;
    margin: 0 0 18px;
    font-size: 13.5px;
    color: var(--ink-2);
  }
  .problem {
    margin: 0 0 14px;
    font-size: 13.5px;
    color: var(--alert);
  }

  .storage {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 980px;
    margin-bottom: 28px;
  }
  .capacity {
    display: flex;
    gap: 2px;
    height: 10px;
    overflow: hidden;
    border-radius: 5px;
    background: var(--surface-2);
  }
  .capacity span {
    display: block;
    transition: width 0.4s var(--ease);
  }
  .is-bloom {
    background: var(--accent);
  }
  .is-other {
    background: color-mix(in oklab, var(--ink-3) 60%, var(--surface-2));
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    font-size: 12px;
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }
  .legend i {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 6px;
    border-radius: 2px;
  }
  .legend .is-free {
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .group {
    max-width: 980px;
    margin-bottom: 30px;
  }
  .group-title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 0 0 4px;
    font-size: 15px;
    font-weight: 600;
  }

  /* Rows ported from the prototype: artwork, what it is and how it's going, then its controls. */
  .dl {
    display: grid;
    grid-template-columns: 132px minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 12px 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .dl:last-child {
    border-bottom: 0;
  }
  .dl-art {
    position: relative;
    display: block;
    width: 132px;
    aspect-ratio: 16 / 9;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: var(--r);
    background: var(--surface);
  }
  .dl-art :global(.art) {
    position: absolute;
    inset: 0;
  }
  button.dl-art {
    cursor: pointer;
  }
  .dl-play {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: #fff;
    background: rgba(0, 0, 0, 0.38);
    opacity: 0;
    transition: opacity 0.18s var(--ease);
  }
  button.dl-art:hover .dl-play,
  button.dl-art:focus-visible .dl-play {
    opacity: 1;
  }
  .dl-progress {
    position: absolute;
    right: 0;
    bottom: 0;
    left: 0;
    height: 3px;
    background: rgba(0, 0, 0, 0.45);
  }
  .dl-progress span {
    display: block;
    height: 100%;
    background: var(--accent-media);
  }

  .dl-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .dl-title,
  .dl-sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dl-title {
    font-size: 14px;
    font-weight: 500;
  }
  .dl-sub {
    font-size: 13px;
    color: var(--ink-2);
  }
  /* Download stats split by hairlines (DESIGN.md §5). */
  .dl-meta {
    display: flex;
    flex-wrap: wrap;
    font-size: 12.5px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .dl-meta span {
    padding-inline: 10px;
    border-left: 1px solid var(--line);
  }
  .dl-meta span:first-child {
    padding-left: 0;
    border-left: 0;
  }
  .dl-error {
    font-size: 12.5px;
    color: var(--alert);
  }
  .dl-bar {
    display: block;
    max-width: 520px;
    margin-top: 4px;
  }

  .dl-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .btn.is-danger {
    border-color: color-mix(in oklab, var(--alert) 50%, var(--line));
    color: var(--alert);
  }

  @media (max-width: 720px) {
    .dl {
      grid-template-columns: 96px minmax(0, 1fr);
    }
    .dl-art {
      width: 96px;
    }
    .dl-actions {
      grid-column: 2;
    }
  }
</style>
