<script lang="ts">
  import { tick, untrack } from "svelte";
  import Mark from "./Mark.svelte";
  import * as api from "./api";

  let {
    lastAddress = null,
    notice = "",
    presetAddress = null,
    onSignedIn,
    onCancel,
  }: {
    lastAddress?: string | null;
    notice?: string;
    /** Adding an account or signing in to a saved server: go straight to that server's sign-in. */
    presetAddress?: string | null;
    onSignedIn: (account: api.Account) => void;
    /** Given, the screen was opened from inside Bloom and can go back. */
    onCancel?: () => void;
  } = $props();

  let saved = $state<api.SavedAccount[]>([]);
  let savedError = $state("");
  let posters = $state<api.CachedPoster[]>([]);
  /** The prototype's wall is four columns of twelve cards; a small cache repeats to fill it. */
  let wall = $derived(posters.length ? Array.from({ length: 16 }, (_, i) => posters[i % posters.length]) : []);

  $effect(() => {
    api
      .cachedPosters()
      .then((list) => (posters = list))
      .catch(() => {});
    untrack(() => {
      if (presetAddress) void checkAddress();
      else {
        if (!onCancel) void loadSaved();
        void lookAround();
      }
    });
  });

  /** Servers that answered on this network; null until the first look has finished. */
  let nearby = $state<api.DiscoveredServer[] | null>(null);
  let looking = $state(false);

  async function lookAround() {
    if (looking) return;
    looking = true;
    try {
      nearby = await api.discoverServers();
    } catch {
      nearby = [];
    } finally {
      looking = false;
    }
  }

  /** A server that answered goes through the same check as a typed address. */
  function connectTo(server: api.DiscoveredServer) {
    address = server.address.replace(/^http:\/\//, "");
    void checkAddress();
  }

  async function loadSaved() {
    try {
      saved = await api.savedAccounts();
    } catch {
      saved = [];
    }
  }

  async function resume(account: api.SavedAccount) {
    if (busy) return;
    busy = true;
    savedError = "";
    try {
      onSignedIn(await api.switchAccount(account.server.id, account.user.id));
    } catch (err) {
      savedError = api.message(err);
      await loadSaved();
    } finally {
      busy = false;
    }
  }

  async function forget(account: api.SavedAccount) {
    if (busy) return;
    busy = true;
    savedError = "";
    try {
      await api.forgetAccount(account.server.id, account.user.id);
      await loadSaved();
    } catch (err) {
      savedError = api.message(err);
    } finally {
      busy = false;
    }
  }

  // Step one: an address. Step two: an account on the server that address answered as.
  let check = $state<api.ServerCheck | null>(null);
  let address = $state("");
  let username = $state("");
  let password = $state("");
  let remember = $state(true);
  let busy = $state(false);
  let error = $state("");

  let qcCode = $state("");
  let qcError = $state("");
  let qcAttempt = $state(0);

  let addressInput = $state<HTMLInputElement>();
  let userInput = $state<HTMLInputElement>();
  let passInput = $state<HTMLInputElement>();

  $effect.pre(() => {
    const start = presetAddress ?? lastAddress;
    if (!address && start) address = start.replace(/^http:\/\//, "");
  });

  function submitAddress(e: SubmitEvent) {
    e.preventDefault();
    void checkAddress();
  }

  async function checkAddress() {
    if (busy) return;
    busy = true;
    error = "";
    try {
      check = await api.checkServer(address);
      await tick();
      userInput?.focus();
    } catch (err) {
      error = api.message(err);
    } finally {
      busy = false;
    }
  }

  async function submitAccount(e: SubmitEvent) {
    e.preventDefault();
    if (busy) return;
    if (!username.trim()) {
      error = "Enter your username.";
      userInput?.focus();
      return;
    }
    busy = true;
    error = "";
    try {
      onSignedIn(await api.signIn(username, password, remember));
    } catch (err) {
      error = api.message(err);
      password = "";
      passInput?.focus();
    } finally {
      busy = false;
    }
  }

  async function differentServer() {
    await api.forgetServer();
    check = null;
    error = "";
    password = "";
    await tick();
    addressInput?.focus();
  }

  function pick(u: api.PublicUser) {
    username = u.name;
    password = "";
    error = "";
    passInput?.focus();
  }

  // Quick Connect runs alongside the password form: a code is requested as soon as the
  // server allows it, and checked every few seconds until approved, expired, or left.
  $effect(() => {
    if (!check?.quickConnect) return;
    void qcAttempt; // "Get a new code" bumps this to restart the loop
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    qcCode = "";
    qcError = "";

    async function poll() {
      try {
        const account = await api.quickConnectPoll(remember);
        if (stopped) return;
        if (account) onSignedIn(account);
        else timer = setTimeout(poll, 3000);
      } catch (err) {
        if (!stopped) {
          qcCode = "";
          qcError = api.message(err);
        }
      }
    }
    (async () => {
      try {
        const code = await api.quickConnectStart();
        if (stopped) return;
        qcCode = code;
        timer = setTimeout(poll, 3000);
      } catch (err) {
        if (!stopped) qcError = api.message(err);
      }
    })();

    return () => {
      stopped = true;
      clearTimeout(timer);
      void api.quickConnectCancel();
    };
  });

  const host = (a: string) => a.replace(/^https?:\/\//, "");
</script>

<div class="auth">
  <div class="auth-form">
    <div class="auth-brand">
      <span class="mark"><Mark size={30} /></span>
      <span class="brandname">Bloom</span>
      {#if onCancel}<button type="button" class="btn back" onclick={onCancel}>Back to Bloom</button>{/if}
    </div>

    {#if notice}<p class="notice" role="status">{notice}</p>{/if}

    {#if !check && saved.length}
      <div class="saved">
        <h1 class="auth-h">Welcome back</h1>
        <div class="saved-list">
          {#each saved as account (`${account.server.id}/${account.user.id}`)}
            <div class="saved-row">
              <span class="mono is-small" aria-hidden="true">{account.user.name.charAt(0).toUpperCase()}</span>
              <span class="saved-text">
                <span class="saved-name">{account.user.name}</span>
                <span class="saved-sub">{account.server.name}, {host(account.server.address)}</span>
              </span>
              <button class="btn btn-primary" disabled={busy} onclick={() => resume(account)}>Continue</button>
              <button
                class="btn"
                disabled={busy}
                aria-label="Forget {account.user.name} on {account.server.name}"
                onclick={() => forget(account)}>Forget</button
              >
            </div>
          {/each}
        </div>
        {#if savedError}<p class="error" role="alert">{savedError}</p>{/if}
      </div>
    {/if}

    {#if !check}
      <form class="stack" onsubmit={submitAddress} novalidate>
        <div>
          {#if saved.length}
            <h2 class="auth-h2">Or connect to a server</h2>
          {:else}
            <h1 class="auth-h">Connect to your server</h1>
          {/if}
          <p class="auth-sub">Pick one Bloom found, or use the address you open Jellyfin with in a browser.</p>
        </div>

        <div class="nearby" aria-live="polite">
          <div class="nearby-head">
            <span class="nearby-label">On your network</span>
            <button type="button" class="link" disabled={looking || busy} onclick={lookAround}>
              {looking ? "Looking" : "Search again"}
            </button>
          </div>
          {#if nearby === null || (looking && !nearby.length)}
            <p class="nearby-note">Looking for Jellyfin servers on this network</p>
          {:else if nearby.length}
            <div class="nearby-list">
              {#each nearby as server (`${server.id} ${server.address}`)}
                <button type="button" class="nearby-row" disabled={busy} onclick={() => connectTo(server)}>
                  <span class="mono is-small" aria-hidden="true">{server.name.charAt(0).toUpperCase()}</span>
                  <span class="saved-text">
                    <span class="saved-name">{server.name}</span>
                    <span class="saved-sub">{host(server.address)}</span>
                  </span>
                  <span class="nearby-go">Connect</span>
                </button>
              {/each}
            </div>
          {:else}
            <p class="nearby-note">
              None answered. A server in Docker without port 7359 published, or on another network, won't show here;
              type its address below.
            </p>
          {/if}
        </div>

        <div class="field">
          <label for="authAddress">Server address</label>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            id="authAddress"
            bind:this={addressInput}
            bind:value={address}
            type="text"
            inputmode="url"
            placeholder="192.168.1.20:8096"
            autocomplete="url"
            spellcheck="false"
            autocapitalize="off"
            autofocus
            aria-invalid={error ? "true" : undefined}
            aria-describedby={error ? "authError" : undefined}
          />
        </div>
        {#if error}<p class="error" id="authError" role="alert">{error}</p>{/if}
        <div class="auth-actions">
          <button class="btn btn-primary" type="submit" disabled={busy}>
            {busy ? "Checking" : "Continue"}
          </button>
        </div>
      </form>
    {:else}
      <form class="stack" onsubmit={submitAccount} novalidate>
        <div>
          <h1 class="auth-h">Sign in to {check.server.name}</h1>
          <p class="auth-sub">
            {host(check.server.address)}, Jellyfin {check.server.version}.
            <button type="button" class="link" onclick={differentServer}>Use a different server</button>
          </p>
        </div>

        <div class="field">
          <label for="authUser">Username</label>
          <input
            id="authUser"
            bind:this={userInput}
            bind:value={username}
            type="text"
            autocomplete="username"
            spellcheck="false"
            autocapitalize="off"
          />
        </div>

        <!-- Most servers hide their user list, so typing a name is the default path. The
             picker only appears when /Users/Public actually returns accounts. -->
        {#if check.users.length}
          <div class="field">
            <span class="label" id="pickLabel">Or pick an account</span>
            <div class="user-grid" role="group" aria-labelledby="pickLabel">
              {#each check.users as u (u.id)}
                <button
                  type="button"
                  class="user-tile"
                  aria-pressed={username === u.name}
                  onclick={() => pick(u)}
                >
                  <span class="mono" aria-hidden="true">{u.name.charAt(0).toUpperCase()}</span>
                  <span class="uname">{u.name}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="field">
          <label for="authPass">Password</label>
          <input
            id="authPass"
            bind:this={passInput}
            bind:value={password}
            type="password"
            autocomplete="current-password"
            aria-describedby="authHint"
          />
          <span class="hint" id="authHint">Leave blank if the account has no password.</span>
        </div>

        {#if error}<p class="error" role="alert">{error}</p>{/if}

        <div class="auth-actions">
          <button class="btn btn-primary" type="submit" disabled={busy}>
            {busy ? "Signing in" : "Sign in"}
          </button>
          <button
            type="button"
            class="switch"
            role="switch"
            aria-checked={remember}
            aria-labelledby="rememberLabel"
            onclick={() => (remember = !remember)}><span></span></button
          >
          <span class="remember" id="rememberLabel">Stay signed in</span>
        </div>

        {#if check.quickConnect}
          <div class="auth-alt">
            <p class="qc-lead">
              No password to hand? On any device already signed in to Jellyfin, open your
              profile, choose Quick Connect, and enter this code.
            </p>
            {#if qcCode}
              <div class="qc-code" aria-label={`Code ${qcCode.split("").join(" ")}`}>{qcCode}</div>
              <p class="qc-note">Signs you in here automatically once approved.</p>
            {:else if qcError}
              <p class="error" role="alert">{qcError}</p>
              <div>
                <button type="button" class="btn" onclick={() => qcAttempt++}>Get a new code</button>
              </div>
            {:else}
              <div class="qc-code is-waiting" aria-hidden="true">000000</div>
            {/if}
          </div>
        {/if}
      </form>
    {/if}
  </div>

  <!-- Painted, so the player underneath stays hidden. Posters come from the artwork cache, which
       needs no request, so they show with nobody signed in; before anything is cached it's plain. -->
  <div class="auth-art" aria-hidden="true">
    {#if wall.length}
      <div class="wall">
        {#each wall as poster, i (i)}
          <img src={api.cachedImageUrl(poster)} alt="" decoding="async" />
        {/each}
      </div>
      <div class="veil"></div>
    {/if}
  </div>
</div>

<style>
  .auth {
    display: grid;
    grid-template-columns: minmax(0, 460px) minmax(0, 1fr);
    height: 100%;
  }
  .auth-form {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 22px;
    padding: 40px clamp(28px, 4vw, 56px);
    background: var(--surface);
    min-width: 0;
    overflow-y: auto;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .auth-brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .mark {
    display: grid;
    color: var(--accent);
  }
  .brandname {
    font-stretch: 118%;
    font-weight: 600;
    letter-spacing: -0.015em;
    font-size: 19px;
  }
  .auth-h {
    margin: 0;
    font-stretch: 118%;
    font-weight: 600;
    letter-spacing: -0.015em;
    font-size: 1.7rem;
    line-height: 1.1;
    overflow-wrap: anywhere;
  }
  .auth-sub {
    margin: 6px 0 0;
    font-size: 13.5px;
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
    color: var(--accent);
    font-weight: 500;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .link:disabled {
    opacity: 0.6;
    cursor: default;
  }

  /* Servers that answered on the network, above the typed address. */
  .nearby {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .nearby-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    font-size: 12.5px;
  }
  .nearby-label {
    font-weight: 500;
    color: var(--ink-2);
  }
  .nearby-note {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-3);
  }
  .nearby-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .nearby-row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-ctl);
    background: var(--ground);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 0.16s var(--ease), border-color 0.16s var(--ease);
  }
  .nearby-row:hover {
    background: var(--surface-2);
  }
  .nearby-row:disabled {
    cursor: default;
    opacity: 0.7;
  }
  .nearby-go {
    font-size: 13px;
    font-weight: 500;
    color: var(--accent);
  }

  .user-grid {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .user-tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 7px;
    width: 78px;
    padding: 9px 4px;
    border: 0;
    border-radius: var(--r);
    background: none;
    cursor: pointer;
    text-align: center;
    transition: background 0.16s var(--ease);
  }
  .user-tile:hover,
  .user-tile[aria-pressed="true"] {
    background: var(--surface-2);
  }
  .mono {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: var(--raise);
    color: var(--ink-2);
    font-stretch: 118%;
    font-weight: 600;
    font-size: 20px;
  }
  .user-tile[aria-pressed="true"] .mono {
    box-shadow: 0 0 0 2px var(--accent);
    color: var(--ink);
  }
  .uname {
    max-width: 100%;
    font-size: 12.5px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .auth-actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .remember {
    font-size: 13px;
    color: var(--ink-2);
  }
  .error {
    margin: 0;
    font-size: 13px;
    color: var(--alert);
  }
  .notice {
    margin: 0;
    padding: 10px 12px;
    font-size: 13px;
    color: var(--ink);
    background: var(--surface-2);
    border-radius: var(--r-ctl);
  }

  .auth-alt {
    border-top: 1px solid var(--line-soft);
    padding-top: 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .qc-lead,
  .qc-note {
    margin: 0;
    font-size: 13px;
    color: var(--ink-2);
  }
  .qc-note {
    font-size: 12px;
    color: var(--ink-3);
  }
  .qc-code {
    font-stretch: 118%;
    font-weight: 600;
    font-size: 2rem;
    line-height: 1.2;
    letter-spacing: 0.12em;
    font-variant-numeric: tabular-nums;
    color: var(--accent);
  }
  .qc-code.is-waiting {
    color: var(--line);
  }

  .auth-brand .back {
    margin-left: auto;
  }
  .auth-h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .saved {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-bottom: 22px;
    border-bottom: 1px solid var(--line-soft);
  }
  .saved-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .saved-row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
  }
  .mono.is-small {
    width: 36px;
    height: 36px;
    font-size: 15px;
  }
  .saved-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }
  .saved-name,
  .saved-sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .saved-name {
    font-size: 14px;
    font-weight: 600;
  }
  .saved-sub {
    font-size: 12.5px;
    color: var(--ink-3);
  }

  .auth-art {
    position: relative;
    overflow: hidden;
    background: var(--ground);
  }
  /* The prototype's right panel: the artwork system at full bleed, four columns tilted and
     scaled past the edges, under a veil that carries the form's surface across. */
  .wall {
    position: absolute;
    inset: -6%;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    align-content: start;
    gap: 14px;
    transform: rotate(-8deg) scale(1.18);
  }
  .wall img {
    display: block;
    width: 100%;
    aspect-ratio: 2 / 3;
    object-fit: cover;
    border-radius: var(--r);
    background: var(--surface);
  }
  .veil {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      100deg,
      var(--surface) 0%,
      color-mix(in oklab, var(--surface) 40%, transparent) 34%,
      transparent 70%
    );
  }
  @media (max-width: 900px) {
    .auth {
      grid-template-columns: minmax(0, 1fr);
    }
    .auth-art {
      display: none;
    }
  }
</style>
