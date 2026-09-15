<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "./api";
  import { bytesLabel } from "./downloads.svelte";

  let {
    onOpenServer,
    onRemovedCurrent,
  }: {
    /** Switch to a sign-in saved for the server, or sign in there. */
    onOpenServer: (entry: api.ServerEntry) => void;
    /** The server in use was removed, which signs Bloom out. */
    onRemovedCurrent: () => void;
  } = $props();

  type Status = "checking" | "reachable" | "unreachable";

  let entries = $state<api.ServerEntry[] | null>(null);
  let status = $state<Record<string, Status>>({});
  let address = $state("");
  let adding = $state(false);
  let addError = $state("");
  let confirming = $state<string | null>(null);
  let removing = $state(false);
  let removeError = $state("");
  /** The downloads from the server being confirmed, which removing it can delete or keep. */
  let confirmDownloads = $state<api.ServerDownloads | null>(null);

  async function askRemove(entry: api.ServerEntry) {
    confirming = entry.server.id;
    confirmDownloads = null;
    removeError = "";
    const id = entry.server.id;
    try {
      const found = await api.serverDownloads(id);
      if (confirming === id) confirmDownloads = found;
    } catch {
      // Not knowing only means the choice isn't offered.
    }
  }

  const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  function lastSeen(seconds: number | null): string {
    if (!seconds) return "never answered";
    const minutes = Math.round((seconds * 1000 - Date.now()) / 60_000);
    if (minutes > -60) return `last seen ${relative.format(minutes, "minute")}`;
    if (minutes > -60 * 24) return `last seen ${relative.format(Math.round(minutes / 60), "hour")}`;
    return `last seen ${relative.format(Math.round(minutes / (60 * 24)), "day")}`;
  }

  async function load() {
    try {
      entries = await api.servers();
    } catch {
      entries = [];
    }
    for (const entry of entries) void ping(entry.server.id);
  }

  async function ping(id: string) {
    status[id] = "checking";
    try {
      const reach = await api.pingServer(id);
      status[id] = reach.reachable ? "reachable" : "unreachable";
      const entry = entries?.find((e) => e.server.id === id);
      if (entry) {
        entry.lastSeen = reach.lastSeen;
        if (reach.version) entry.server.version = reach.version;
      }
    } catch {
      status[id] = "unreachable";
    }
  }

  $effect(() => {
    untrack(() => void load());
  });

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (adding) return;
    adding = true;
    addError = "";
    try {
      await api.checkServer(address);
      address = "";
      await load();
    } catch (err) {
      addError = api.message(err);
    } finally {
      adding = false;
    }
  }

  /** Kept downloads stay on disk and come back if the server is added again. */
  async function remove(entry: api.ServerEntry, deleteDownloads: boolean) {
    removing = true;
    removeError = "";
    try {
      if (deleteDownloads) await api.removeServerDownloads(entry.server.id);
      const signedOut = await api.removeServer(entry.server.id);
      confirming = null;
      if (signedOut) onRemovedCurrent();
      else await load();
    } catch (err) {
      removeError = api.message(err);
    } finally {
      removing = false;
    }
  }

  function stateLabel(entry: api.ServerEntry, s: Status | undefined): { text: string; tone: "" | "ok" | "bad" } {
    if (!s || s === "checking") return { text: "Checking", tone: "" };
    if (s === "unreachable") {
      return { text: entry.current ? "Can't reach it" : `No response, ${lastSeen(entry.lastSeen)}`, tone: "bad" };
    }
    return entry.current ? { text: "Connected", tone: "ok" } : { text: "Reachable", tone: "" };
  }
</script>

<div class="page">
  <header class="page-head">
    <div class="page-heading">
      <h1 class="page-title">Servers</h1>
      <span class="eyebrow">Every Jellyfin server added on this computer, and the accounts saved for each.</span>
    </div>
  </header>

  <div class="servers">
    {#if entries === null}
      <div class="srv" aria-busy="true" aria-label="Loading servers"><div class="srv-row is-loading"></div></div>
    {:else if entries.length === 0}
      <p class="none">No servers yet. Add one by its address below.</p>
    {:else}
      <div class="srv">
        {#each entries as entry (entry.server.id)}
          {@const s = status[entry.server.id]}
          {@const label = stateLabel(entry, s)}
          <div class="srv-row">
            <span class="mono" aria-hidden="true">{entry.server.name.charAt(0).toUpperCase()}</span>
            <span class="srv-text">
              <span class="srv-name">{entry.server.name}</span>
              <span class="srv-meta">
                {entry.server.address.replace(/^https?:\/\//, "")}, Jellyfin {entry.server.version}{entry.accounts.length
                  ? `, saved for ${entry.accounts.map((u) => u.name).join(", ")}`
                  : ""}
              </span>
            </span>
            <span class="srv-right">
              <span class="srv-state" class:ok={label.tone === "ok"} class:bad={label.tone === "bad"} role="status">{label.text}</span>
              {#if confirming === entry.server.id && confirmDownloads?.count}
                <button class="btn danger" disabled={removing} onclick={() => remove(entry, true)}>Remove and delete them</button>
                <button class="btn" disabled={removing} onclick={() => remove(entry, false)}>Remove and keep them</button>
                <button class="btn" disabled={removing} onclick={() => (confirming = null)}>Cancel</button>
              {:else if confirming === entry.server.id}
                <button class="btn danger" disabled={removing} onclick={() => remove(entry, false)}>
                  {entry.accounts.length || entry.current ? "Remove and sign out" : "Remove"}
                </button>
                <button class="btn" disabled={removing} onclick={() => (confirming = null)}>Keep</button>
              {:else}
                {#if s === "unreachable"}
                  <button class="btn" onclick={() => ping(entry.server.id)}>Retry</button>
                {:else if !entry.current}
                  <button class="btn" disabled={s !== "reachable"} onclick={() => onOpenServer(entry)}>
                    {entry.accounts.length ? "Switch" : "Sign in"}
                  </button>
                {/if}
                <button class="btn" onclick={() => askRemove(entry)}>Remove</button>
              {/if}
            </span>
            {#if confirming === entry.server.id && confirmDownloads?.count}
              {@const count = confirmDownloads.count}
              <p class="srv-confirm" role="status">
                {count} {count === 1 ? "download" : "downloads"} ({bytesLabel(confirmDownloads.bytes)}) came from
                {entry.server.name}{entry.accounts.length || entry.current ? ", and removing it signs out" : ""}. Kept, they
                stay on disk and come back if you add it again.
              </p>
            {/if}
          </div>
        {/each}
      </div>
      {#if removeError}<p class="error" role="alert">{removeError}</p>{/if}
    {/if}

    <form class="srv-add" onsubmit={add} novalidate>
      <div class="field">
        <label for="server-address">Add a server</label>
        <input
          id="server-address"
          type="text"
          inputmode="url"
          placeholder="192.168.1.20:8096"
          autocomplete="off"
          spellcheck="false"
          autocapitalize="off"
          bind:value={address}
          aria-invalid={addError ? "true" : undefined}
        />
      </div>
      <button class="btn btn-primary" type="submit" disabled={adding || !address.trim()}>{adding ? "Checking" : "Add"}</button>
    </form>
    {#if addError}<p class="error" role="alert">{addError}</p>{/if}
  </div>
</div>

<style>
  .servers {
    max-width: 860px;
  }
  .srv {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--line-soft);
  }
  .srv-row {
    display: grid;
    grid-template-columns: 38px minmax(0, 1fr) auto;
    align-items: center;
    gap: 14px;
    padding: 14px 4px;
    border-bottom: 1px solid var(--line-soft);
  }
  .srv-row.is-loading {
    height: 68px;
  }
  .mono {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border-radius: var(--r);
    background: var(--surface-2);
    color: var(--ink-2);
    font-stretch: 118%;
    font-weight: 600;
    font-size: 16px;
  }
  .srv-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .srv-name {
    font-size: 14px;
    font-weight: 600;
  }
  .srv-meta {
    font-size: 12.5px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
  .srv-right {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 8px 10px;
  }
  .srv-state {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--ink-3);
  }
  .srv-state.ok {
    color: var(--good);
  }
  .srv-state.bad {
    color: var(--alert);
  }
  /* Under the row, across it: what removing does to the server's downloads. */
  .srv-confirm {
    grid-column: 2 / -1;
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-2);
  }
  .btn.danger {
    color: var(--alert);
    border-color: color-mix(in oklab, var(--alert) 45%, var(--line));
  }
  .none {
    margin: 0;
    font-size: 14px;
    color: var(--ink-2);
  }
  .srv-add {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px;
    margin-top: 24px;
  }
  .srv-add .field {
    flex: 1 1 280px;
  }
  .error {
    margin: 10px 0 0;
    font-size: 13px;
    color: var(--alert);
  }
  @media (max-width: 700px) {
    .srv-row {
      grid-template-columns: 38px minmax(0, 1fr);
    }
    .srv-right {
      grid-column: 1 / -1;
      justify-content: flex-start;
    }
  }
</style>
