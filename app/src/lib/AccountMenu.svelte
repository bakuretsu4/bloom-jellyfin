<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import * as api from "./api";

  let {
    account,
    busy = false,
    onSignOut,
    onSwitch,
    onAddAccount,
    onShortcuts,
  }: {
    account: api.Account;
    busy?: boolean;
    onSignOut: () => void;
    /** Another saved sign-in on this server. */
    onSwitch: (to: api.SavedAccount) => void;
    onAddAccount: () => void;
    onShortcuts: () => void;
  } = $props();

  let open = $state(false);
  let others = $state<api.SavedAccount[]>([]);
  let anchor = $state<HTMLDivElement>();
  let button = $state<HTMLButtonElement>();

  $effect(() => {
    if (!open) return;
    api
      .savedAccounts()
      .then((list) => (others = list.filter((a) => !a.current && a.server.id === account.server.id)))
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
    class="iconbtn"
    bind:this={button}
    aria-label="Account: {account.user.name}"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <Icon name="user" />
  </button>

  {#if open}
    <div class="popover" role="menu" aria-label="Account">
      <div class="acct">
        <span class="mono" aria-hidden="true">{account.user.name.charAt(0).toUpperCase()}</span>
        <span class="acct-text">
          <span class="acct-name">{account.user.name}</span>
          <span class="acct-sub">Signed in on {account.server.name}</span>
        </span>
      </div>
      {#if others.length}
        <div class="group-label">Switch to</div>
        {#each others as other (other.user.id)}
          <button class="pop-opt" role="menuitem" onclick={() => choose(() => onSwitch(other))}>
            <span class="who">
              <span class="mono is-small" aria-hidden="true">{other.user.name.charAt(0).toUpperCase()}</span>
              {other.user.name}
            </span>
          </button>
        {/each}
      {/if}
      <div class="divider"></div>
      <button class="pop-opt" role="menuitem" onclick={() => choose(onShortcuts)}>
        Keyboard shortcuts<kbd>?</kbd>
      </button>
      <button class="pop-opt" role="menuitem" onclick={() => choose(onAddAccount)}>
        Add another account<Icon name="user" size={16} />
      </button>
      <button class="pop-opt" role="menuitem" disabled={busy} onclick={() => choose(onSignOut)}>
        {busy ? "Signing out" : "Sign out"}<Icon name="power" size={16} />
      </button>
    </div>
  {/if}
</div>

<style>
  .menu-anchor {
    position: relative;
  }
  .popover {
    position: absolute;
    right: 0;
    top: calc(100% + 9px);
    z-index: 30;
    width: 250px;
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
  .acct {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px 11px;
    margin-bottom: 6px;
    border-bottom: 1px solid var(--line-soft);
    min-width: 0;
  }
  .mono {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: var(--r);
    background: var(--surface-2);
    color: var(--ink-2);
    font-stretch: 118%;
    font-weight: 600;
    font-size: 15px;
  }
  .mono.is-small {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    font-size: 12px;
  }
  .acct-text {
    min-width: 0;
  }
  .acct-name,
  .acct-sub {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acct-name {
    font-size: 13.5px;
    font-weight: 600;
  }
  .acct-sub {
    font-size: 12px;
    color: var(--ink-3);
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
  .who {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
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
    cursor: pointer;
    font-size: 13px;
    color: var(--ink-2);
    text-align: left;
    transition: background 0.16s var(--ease), color 0.16s var(--ease);
  }
  .pop-opt:hover,
  .pop-opt:focus-visible {
    background: var(--surface-2);
    color: var(--ink);
  }
  .pop-opt:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .pop-opt :global(svg) {
    color: var(--ink-3);
  }
  kbd {
    padding: 1px 5px;
    border: 1px solid var(--line);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--surface);
    color: var(--ink);
    font-family: inherit;
    font-size: 10.5px;
  }
</style>
