<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import * as api from "./api";

  let {
    server,
    onOpenServer,
    onManage,
  }: {
    server: api.Server;
    /** Another saved server: switch to a sign-in saved for it, or sign in there. */
    onOpenServer: (entry: api.ServerEntry) => void;
    onManage: () => void;
  } = $props();

  let open = $state(false);
  let others = $state<api.ServerEntry[]>([]);
  let anchor = $state<HTMLDivElement>();
  let button = $state<HTMLButtonElement>();

  let initial = $derived(server.name.charAt(0).toUpperCase());
  let host = $derived(server.address.replace(/^https?:\/\//, ""));

  $effect(() => {
    if (!open) return;
    api
      .servers()
      .then((list) => (others = list.filter((e) => !e.current)))
      .catch(() => (others = []));
    tick().then(() => anchor?.querySelector<HTMLElement>('[role="menuitem"]')?.focus());
    const onPointer = (e: PointerEvent) => {
      if (!anchor?.contains(e.target as Node)) open = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        open = false;
        button?.focus();
      }
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  });

  function choose(action: () => void) {
    open = false;
    action();
  }
</script>

<div class="menu-anchor" bind:this={anchor}>
  <button
    class="serverbtn"
    bind:this={button}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label="Server: {server.name}"
    onclick={() => (open = !open)}
  >
    <span class="mono" aria-hidden="true">{initial}</span>
    <span class="server-name">{server.name}</span>
    <span class="caret" aria-hidden="true"></span>
  </button>

  {#if open}
    <div class="popover" role="menu" aria-label="Server">
      <div class="head">
        <span class="mono is-large" aria-hidden="true">{initial}</span>
        <span class="head-text">
          <span class="head-name">{server.name}</span>
          <span class="head-sub">{host}, Jellyfin {server.version}</span>
        </span>
      </div>
      {#if others.length}
        <div class="group-label">Other servers</div>
        {#each others as entry (entry.server.id)}
          <button class="pop-opt" role="menuitem" onclick={() => choose(() => onOpenServer(entry))}>
            <span class="row-text">
              <span class="row-name">{entry.server.name}</span>
              <span class="row-sub">{entry.accounts.length ? `As ${entry.accounts[0].name}` : "Sign in"}</span>
            </span>
            <Icon name="chevr" size={14} />
          </button>
        {/each}
        <div class="divider"></div>
      {/if}
      <button class="pop-opt" role="menuitem" onclick={() => choose(onManage)}>
        Manage servers<Icon name="chevr" size={14} />
      </button>
    </div>
  {/if}
</div>

<style>
  .menu-anchor {
    position: relative;
  }
  /* Connected is the expected state, so it's deliberately unstyled. */
  .serverbtn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    max-width: 220px;
    padding: 0 9px 0 7px;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--ink-2);
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.16s var(--ease), color 0.16s var(--ease);
  }
  .serverbtn:hover,
  .serverbtn[aria-expanded="true"] {
    background: var(--surface-2);
    color: var(--ink);
  }
  .server-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caret {
    flex: none;
    width: 0;
    height: 0;
    margin-top: 3px;
    border: 4px solid transparent;
    border-top-color: currentColor;
    opacity: 0.55;
  }
  .mono {
    display: grid;
    place-items: center;
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--ink-2);
    font-stretch: 118%;
    font-weight: 600;
    font-size: 11.5px;
  }
  .mono.is-large {
    width: 36px;
    height: 36px;
    border-radius: var(--r);
    font-size: 15px;
  }
  .popover {
    position: absolute;
    right: 0;
    top: calc(100% + 9px);
    z-index: 30;
    width: 280px;
    padding: 8px;
    background: var(--md-sys-color-surface-container);
    border-radius: var(--md-sys-shape-lg);
    box-shadow: var(--md-sys-elevation-2);
    animation: pop 0.2s var(--ease) both;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 4px 8px 11px;
    margin-bottom: 6px;
    border-bottom: 1px solid var(--line-soft);
  }
  .head-text,
  .row-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .head-name,
  .head-sub,
  .row-name,
  .row-sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head-name {
    font-size: 13.5px;
    font-weight: 600;
  }
  .head-sub,
  .row-sub {
    font-size: 12px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .row-name {
    font-size: 13px;
    color: var(--ink);
  }
  .group-label {
    padding: 4px 8px;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .divider {
    height: 1px;
    margin: 6px 0;
    background: var(--line-soft);
  }
  .pop-opt {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--ink-2);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition: background 0.16s var(--ease), color 0.16s var(--ease);
  }
  .pop-opt:hover,
  .pop-opt:focus-visible {
    background: var(--surface-2);
    color: var(--ink);
  }
  .pop-opt :global(svg) {
    flex: none;
    color: var(--ink-3);
  }
</style>
