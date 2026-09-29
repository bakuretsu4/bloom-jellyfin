<script lang="ts">
  import { tick, untrack } from "svelte";
  import AccountMenu from "./lib/AccountMenu.svelte";
  import CommandPalette, { type Place } from "./lib/CommandPalette.svelte";
  import DownloadsPage from "./lib/DownloadsPage.svelte";
  import { downloads as downloadList, trackDownloads } from "./lib/downloads.svelte";
  import { clearLive, installLive } from "./lib/live.svelte";
  import Home from "./lib/Home.svelte";
  import Icon from "./lib/Icon.svelte";
  import ItemPage from "./lib/ItemPage.svelte";
  import LibrariesPage from "./lib/LibrariesPage.svelte";
  import LibraryPage from "./lib/LibraryPage.svelte";
  import Mark from "./lib/Mark.svelte";
  import SearchBox from "./lib/SearchBox.svelte";
  import SearchPage from "./lib/SearchPage.svelte";
  import ServerMenu from "./lib/ServerMenu.svelte";
  import ServersPage from "./lib/ServersPage.svelte";
  import SettingsPage from "./lib/SettingsPage.svelte";
  import ShortcutsOverlay from "./lib/ShortcutsOverlay.svelte";
  import { clearCardDetails } from "./lib/hover.svelte";
  import Launch from "./lib/Launch.svelte";
  import { morph, takeSource } from "./lib/motion";
  import { reducedMotion } from "./lib/settings.svelte";
  import { loadSettings } from "./lib/settings.svelte";
  import SignIn from "./lib/SignIn.svelte";
  import Watch from "./lib/Watch.svelte";
  import * as api from "./lib/api";
  import { installZoom, view } from "./lib/zoom.svelte";

  type Route =
    | { name: "home" }
    | { name: "libraries" }
    | { name: "library"; id: string; title: string }
    | { name: "item"; id: string }
    | { name: "search"; query: string }
    | { name: "settings" }
    | { name: "servers" }
    | { name: "downloads" };

  const PLAYS_DIRECTLY = new Set(["Episode", "Video", "MusicVideo"]);
  const HOME: Route = { name: "home" };

  let phase = $state<"starting" | "signin" | "signedin" | "browser">("starting");
  let account = $state<api.Account | null>(null);
  let lastAddress = $state<string | null>(null);
  let notice = $state("");
  let signingOut = $state(false);
  /** The saved session's server didn't answer at startup. */
  let offline = $state(false);
  /** The sign-in screen was opened from inside Bloom (another account or server), so it can go back. */
  let addingAccount = $state(false);
  let signInAddress = $state<string | null>(null);
  let shortcutsOpen = $state(false);
  /** Ctrl+K: jump to a title, library, setting or page. */
  let paletteOpen = $state(false);
  /** The launch roll is on screen; `afterCover` runs once it hides the window. */
  let launching = $state(false);
  let afterCover: (() => void) | null = null;
  /** A problem with an action in the shell, such as a switch that failed. */
  let shellNotice = $state("");

  /** The item playing, while the watch screen is up. The page underneath stays in history. */
  let watching = $state<string | null>(null);
  /** While the watch screen plays a trailer: which of the item's trailers. */
  let trailerIndex = $state<number | null>(null);
  let history = $state<Route[]>([HOME]);
  let railCollapsed = $state(false);
  /** Bumped after playback so Home's rows reload. */
  let homeRefresh = $state(0);
  let query = $state("");
  let stage = $state<HTMLElement>();
  /** The page has scrolled under the top bar, which then takes a tonal surface (M3). */
  let scrolled = $state(false);
  let searchBox = $state<{ focus: () => void }>();

  let route = $derived(history[history.length - 1]);
  let libraryRoute = $derived(route.name === "library" ? route : null);
  let itemRoute = $derived(route.name === "item" ? route : null);
  let searchRoute = $derived(route.name === "search" ? route : null);

  const scrollPositions = new Map<string, number>();
  /** Remembers where a page was scrolled; only the most recent pages, so a long session doesn't pile up. */
  function rememberScroll(key: string, top: number) {
    scrollPositions.delete(key);
    scrollPositions.set(key, top);
    if (scrollPositions.size > 60) scrollPositions.delete(scrollPositions.keys().next().value!);
  }
  const keyOf = (r: Route) => JSON.stringify(r);

  $effect(() => {
    // Settings alongside, so the saved theme and accent are in place before the first screen paints.
    Promise.all([api.startup(), loadSettings().catch(() => {})])
      .then(([s]) => {
        lastAddress = s.lastAddress;
        account = s.account;
        offline = s.offline;
        phase = s.account ? "signedin" : "signin";
      })
      .catch(() => (phase = "browser"));
  });

  $effect(() => installZoom());

  // The downloads list belongs to the account in use.
  $effect(() => {
    const key = account ? `${account.server.id}/${account.user.id}` : null;
    if (key) untrack(() => void trackDownloads());
  });
  let downloading = $derived(downloadList.list.some((d) => d.status === "downloading"));
  let hasDownloaded = $derived(downloadList.list.some((d) => d.status === "done"));

  // The server's live updates. Home's rows reload a moment after the last of a burst of changes,
  // so a whole season marked watched is one reload. Watched marks and progress show on the cards
  // straight away (live.svelte.ts), so for those the rows reload at most once a minute: another
  // device playing reports its progress every ten seconds. While watching they wait: closing the
  // player reloads them anyway.
  const LIVE_RELOAD_GAP_MS = 60_000;
  let liveReload: ReturnType<typeof setTimeout> | undefined;
  let lastLiveReload = 0;
  $effect(() => {
    installLive((what) => {
      clearTimeout(liveReload);
      const wait = what === "library" ? 2500 : Math.max(2500, lastLiveReload + LIVE_RELOAD_GAP_MS - Date.now());
      liveReload = setTimeout(() => {
        if (watching) return;
        lastLiveReload = Date.now();
        homeRefresh++;
      }, wait);
    });
  });

  // A short readout after each zoom step, like a browser's.
  let zoomShown = $state(false);
  $effect(() => {
    if (view.flash === 0) return;
    zoomShown = true;
    const t = setTimeout(() => (zoomShown = false), 1200);
    return () => clearTimeout(t);
  });

  // --- navigation

  /** Replace the history, keeping each page's scroll position for when it comes back. */
  async function show(next: Route[]) {
    if (stage && !watching) rememberScroll(keyOf(route), stage.scrollTop);
    history = next;
    await tick();
    if (stage) stage.scrollTop = scrollPositions.get(keyOf(next[next.length - 1])) ?? 0;
  }

  async function navigate(next: Route, replace = false) {
    await leaveWatch();
    if (keyOf(next) === keyOf(route)) return;
    await show(replace ? [...history.slice(0, -1), next] : [...history, next]);
  }

  async function goBack() {
    if (watching || history.length < 2) return;
    await show(history.slice(0, -1));
  }

  /** The rail's destinations start a fresh trail from Home. */
  async function goSection(section: Route) {
    await leaveWatch();
    await show(section.name === "home" ? [HOME] : [HOME, section]);
  }

  function open(card: { id: string; kind: string; title: string }) {
    if (card.kind === "CollectionFolder" || card.kind === "UserView") {
      takeSource();
      void navigate({ name: "library", id: card.id, title: card.title });
    } else if (card.kind === "Series" || card.kind === "Movie" || card.kind === "BoxSet") {
      // The poster grows into the title's page.
      void morph(takeSource(), () => navigate({ name: "item", id: card.id }));
    } else if (PLAYS_DIRECTLY.has(card.kind)) {
      play(card.id);
    }
  }

  // --- playback

  function play(itemId: string) {
    if (stage) rememberScroll(keyOf(route), stage.scrollTop);
    // The artwork grows into the player.
    void morph(takeSource(), () => {
      trailerIndex = null;
      watching = itemId;
    });
  }

  /** An item's trailer on the watch screen. */
  function watchTrailer(itemId: string, index: number) {
    if (stage) rememberScroll(keyOf(route), stage.scrollTop);
    trailerIndex = index;
    watching = itemId;
  }

  // --- the launch roll

  /** Signal acquire, after signing in. `then` swaps the screen while the roll hides it. */
  function playLaunch(then?: () => void) {
    if (reducedMotion()) {
      then?.();
      return;
    }
    afterCover = then ?? null;
    // Replaying restarts it.
    launching = false;
    void tick().then(() => (launching = true));
  }

  function launchCovered() {
    const run = afterCover;
    afterCover = null;
    run?.();
  }

  function watchClosed() {
    watching = null;
    trailerIndex = null;
    homeRefresh++;
    void tick().then(() => {
      if (stage) stage.scrollTop = scrollPositions.get(keyOf(route)) ?? 0;
    });
  }

  /** The watch screen's link to the show: its page, or back to it when that's the page underneath. */
  async function openFromWatch(itemId: string) {
    await leaveWatch();
    await tick();
    if (stage) stage.scrollTop = scrollPositions.get(keyOf(route)) ?? 0;
    await navigate({ name: "item", id: itemId });
  }

  /** Leaving the watch screen some other way than its own Back: stop, and let the server hear. */
  async function leaveWatch() {
    if (!watching) return;
    await api.stopPlayback().catch(() => {});
    watching = null;
    trailerIndex = null;
    homeRefresh++;
  }

  // --- search

  /** The full results page. A new search replaces the one open rather than stacking pages. */
  function searchFor(q: string) {
    if (q.length < 2) {
      if (route.name === "search") void goBack();
      return;
    }
    void navigate({ name: "search", query: q }, route.name === "search");
  }

  function clearSearch() {
    query = "";
    if (route.name === "search") void goBack();
  }

  // --- keys and the mouse's back button

  function onKey(e: KeyboardEvent) {
    if (phase !== "signedin") return;
    const target = e.target as HTMLElement | null;
    const typing =
      target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "k") {
      e.preventDefault();
      paletteOpen = !paletteOpen;
      return;
    }
    if (e.key === "?" && !typing && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      shortcutsOpen = true;
      return;
    }
    if (watching) return;
    if (e.key === "/" && !typing && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      searchBox?.focus();
    } else if (e.altKey && e.key === "ArrowLeft") {
      e.preventDefault();
      void goBack();
    }
  }

  function onMouseUp(e: MouseEvent) {
    if (e.button === 3 && phase === "signedin" && !watching) {
      e.preventDefault();
      void goBack();
    }
  }

  // --- the command palette

  /** Leaves the player first: the watch screen only plays what it opened with. */
  async function paletteTitle(card: api.Card) {
    await leaveWatch();
    await tick();
    open(card);
  }

  function paletteGo(place: Place) {
    void goSection(place === "home" ? HOME : { name: place });
  }

  async function paletteLibrary(library: api.Card) {
    await goSection({ name: "libraries" });
    await navigate({ name: "library", id: library.id, title: library.title });
  }

  async function paletteSettings(sectionId: string) {
    await goSection({ name: "settings" });
    await tick();
    document.getElementById(sectionId)?.scrollIntoView({ block: "start" });
  }

  // --- session

  /** Into the shell as `next`, from sign-in or a switch. The shell is keyed by the account, so
   *  every page starts over as that user. */
  function enterAccount(next: api.Account) {
    resetShell();
    account = next;
    lastAddress = next.server.address;
    notice = "";
    phase = "signedin";
  }

  function signedIn(a: api.Account) {
    playLaunch(() => enterAccount(a));
  }

  function resetShell() {
    account = null;
    watching = null;
    history = [HOME];
    query = "";
    scrollPositions.clear();
    clearCardDetails();
    clearLive();
    offline = false;
    paletteOpen = false;
    addingAccount = false;
    signInAddress = null;
    shellNotice = "";
  }

  /** Another saved sign-in. */
  async function switchTo(target: api.SavedAccount) {
    await leaveWatch();
    shellNotice = "";
    try {
      enterAccount(await api.switchAccount(target.server.id, target.user.id));
    } catch (e) {
      shellNotice = api.message(e);
    }
  }

  /** Sign-in for another account, on `address`, keeping the current one to come back to. */
  async function signInElsewhere(address: string) {
    await leaveWatch();
    signInAddress = address;
    addingAccount = true;
    phase = "signin";
  }

  function cancelSignIn() {
    addingAccount = false;
    signInAddress = null;
    phase = "signedin";
  }

  async function openServer(entry: api.ServerEntry) {
    if (entry.current) return;
    const saved = (await api.savedAccounts().catch(() => [] as api.SavedAccount[])).find((a) => a.server.id === entry.server.id);
    if (saved) await switchTo(saved);
    else await signInElsewhere(entry.server.address);
  }

  /** The server in use was removed from the Servers screen. */
  function currentRemoved() {
    resetShell();
    phase = "signin";
  }

  async function retryServer() {
    if (!account) return;
    const reach = await api.pingServer(account.server.id).catch(() => null);
    if (reach?.reachable) {
      offline = false;
      homeRefresh++;
      // Where downloads were watched while it was away.
      void api.syncDownloads().catch(() => {});
    } else {
      shellNotice = `${account.server.name} still isn't answering.`;
    }
  }

  // The server stopped accepting the token (signed out from the dashboard, password changed).
  function sessionEnded() {
    resetShell();
    notice = "Your sign-in ended on the server. Sign in again to carry on.";
    phase = "signin";
  }

  async function signOut() {
    if (signingOut) return;
    signingOut = true;
    try {
      // Stop first, while the sign-in still stands, so the server hears where playback stopped.
      await leaveWatch();
      await api.signOut();
    } finally {
      signingOut = false;
      resetShell();
      notice = "";
      phase = "signin";
    }
  }

</script>

<svelte:window onkeydown={onKey} onmouseup={onMouseUp} />

{#if phase === "starting"}
  <!-- Painted, so the player underneath doesn't show before the first screen. -->
  <div class="blank"></div>
{:else if phase === "browser"}
  <div class="blank note">
    Running in a browser, so there is no server connection or player. Launch with
    <code>npm run tauri dev</code>.
  </div>
{:else if phase === "signin"}
  <SignIn
    {lastAddress}
    {notice}
    presetAddress={signInAddress}
    onSignedIn={signedIn}
    onCancel={addingAccount && account ? cancelSignIn : undefined}
  />
{:else if account}
  {#key `${account.server.id}/${account.user.id}`}
  <div class="shell" class:rail-collapsed={railCollapsed} class:is-watching={!!watching}>
    <header class="topbar" class:is-scrolled={scrolled}>
      <div class="brand">
        <button
          class="iconbtn"
          aria-label={railCollapsed ? "Show navigation" : "Hide navigation"}
          aria-expanded={!railCollapsed}
          aria-controls="rail"
          onclick={() => (railCollapsed = !railCollapsed)}
        >
          <Icon name="menu" />
        </button>
        <span class="mark"><Mark size={24} /></span>
        <span class="wordmark">Bloom</span>
      </div>

      <SearchBox bind:this={searchBox} bind:query onOpen={open} onSearch={searchFor} onSignedOut={sessionEnded} />

      <div class="tools">
        <ServerMenu server={account.server} onOpenServer={openServer} onManage={() => goSection({ name: "servers" })} />
        <AccountMenu
          {account}
          busy={signingOut}
          onSignOut={signOut}
          onSwitch={switchTo}
          onAddAccount={() => account && signInElsewhere(account.server.address)}
          onShortcuts={() => (shortcutsOpen = true)}
        />
      </div>
    </header>

    <!-- Only destinations that exist. Material 3 navigation rail: a pill behind the icon marks
         the current place, and its icon is the filled form. -->
    <nav class="rail" id="rail" aria-label="Sections">
      <div class="rail-top">
        {#each [
          { id: "home", label: "Home", icon: "home", active: !watching && route.name === "home", go: () => goSection(HOME) },
          { id: "library", label: "Library", icon: "library", active: !watching && (route.name === "libraries" || route.name === "library"), go: () => goSection({ name: "libraries" }) },
          { id: "downloads", label: "Downloads", icon: "download", active: !watching && route.name === "downloads", go: () => goSection({ name: "downloads" }) },
        ] as const as item (item.id)}
          <button
            class="navitem"
            class:is-active={item.active}
            aria-label={item.id === "downloads" && downloading ? "Downloads, downloading now" : undefined}
            aria-current={item.active ? "page" : undefined}
            onclick={item.go}
          >
            <span class="indicator" data-ripple>
              <Icon name={item.icon} filled={item.active} />
              {#if item.id === "downloads" && downloading}<span class="dot" aria-hidden="true"></span>{/if}
            </span>
            <span class="navlabel">{item.label}</span>
          </button>
        {/each}
        {#if watching}
          <button class="navitem is-active" aria-current="page">
            <span class="indicator"><Icon name="play-circle" filled /></span>
            <span class="navlabel">Playing</span>
          </button>
        {/if}
      </div>
      <div class="rail-bottom">
        <button class="navitem" aria-label="Keyboard shortcuts (?)" aria-haspopup="dialog" onclick={() => (shortcutsOpen = true)}>
          <span class="indicator" data-ripple><Icon name="help" /></span>
          <span class="navlabel">Shortcuts</span>
        </button>
        <button
          class="navitem"
          class:is-active={!watching && route.name === "settings"}
          aria-current={!watching && route.name === "settings" ? "page" : undefined}
          onclick={() => goSection({ name: "settings" })}
        >
          <span class="indicator" data-ripple>
            <Icon name="gear" filled={!watching && route.name === "settings"} />
          </span>
          <span class="navlabel">Settings</span>
        </button>
      </div>
    </nav>

    <main class="stage" class:is-watching={!!watching} bind:this={stage} onscroll={(e) => (scrolled = e.currentTarget.scrollTop > 4)}>
      {#if !watching && offline}
        <div class="banner" role="status">
          <span>
            Can't reach {account.server.name}. Pages and artwork you've already opened may still show{hasDownloaded
              ? ", and everything downloaded still plays"
              : ""}.
          </span>
          <span class="banner-actions">
            {#if hasDownloaded && route.name !== "downloads"}
              <button class="btn" onclick={() => goSection({ name: "downloads" })}>Browse downloads</button>
            {/if}
            <button class="btn" onclick={retryServer}>Retry</button>
          </span>
        </div>
      {/if}
      {#if !watching && shellNotice}
        <div class="banner is-alert" role="alert">
          <span>{shellNotice}</span>
          <button class="btn" onclick={() => (shellNotice = "")}>Dismiss</button>
        </div>
      {/if}
      <!-- Home stays alive under other pages, so its spotlight and scroll survive a round trip. -->
      <div hidden={!!watching || route.name !== "home"}>
        <Home onSignedOut={sessionEnded} onOpen={open} onPlay={play} refreshToken={homeRefresh} />
      </div>

      {#if watching}
        <Watch
          itemId={watching}
          {trailerIndex}
          onClose={watchClosed}
          onSignedOut={sessionEnded}
          onOpenItem={openFromWatch}
          onOpenDownloads={() => goSection({ name: "downloads" })}
        />
      {:else if route.name === "libraries"}
        <LibrariesPage onOpen={open} onSignedOut={sessionEnded} />
      {:else if libraryRoute}
        {#key libraryRoute.id}
          <LibraryPage
            id={libraryRoute.id}
            title={libraryRoute.title}
            onBack={goBack}
            onOpen={open}
            onPlay={play}
            onSignedOut={sessionEnded}
          />
        {/key}
      {:else if itemRoute}
        {#key itemRoute.id}
          <ItemPage
            id={itemRoute.id}
            onBack={goBack}
            onPlay={play}
            onOpen={open}
            onSignedOut={sessionEnded}
            onOpenDownloads={() => goSection({ name: "downloads" })}
            onTrailer={watchTrailer}
          />
        {/key}
      {:else if route.name === "downloads"}
        <DownloadsPage serverName={account.server.name} {offline} onPlay={play} />
      {:else if route.name === "servers"}
        <ServersPage onOpenServer={openServer} onRemovedCurrent={currentRemoved} />
      {:else if route.name === "settings"}
        <SettingsPage {account} />
      {:else if searchRoute}
        {#key searchRoute.query}
          <SearchPage query={searchRoute.query} onOpen={open} onPlay={play} onClear={clearSearch} onSignedOut={sessionEnded} />
        {/key}
      {/if}
    </main>
  </div>
  {/key}
{/if}

{#if shortcutsOpen}<ShortcutsOverlay onClose={() => (shortcutsOpen = false)} />{/if}

{#if paletteOpen && phase === "signedin" && account}
  <CommandPalette
    onClose={() => (paletteOpen = false)}
    onOpenTitle={paletteTitle}
    onGo={paletteGo}
    onLibrary={paletteLibrary}
    onSettings={paletteSettings}
    onShortcuts={() => (shortcutsOpen = true)}
    onSignedOut={sessionEnded}
  />
{/if}

{#if launching}<Launch onCovered={launchCovered} onDone={() => (launching = false)} />{/if}

<div class="zoom-readout" role="status" hidden={!zoomShown}>{Math.round(view.zoom * 100)}%</div>

<style>
  .blank {
    height: 100%;
    background: var(--ground);
  }
  .note {
    display: grid;
    place-content: center;
    padding: 24px;
    color: var(--ink-2);
    text-align: center;
  }

  .shell {
    --rail-w: 80px;
    --bar-h: 64px;
    display: grid;
    grid-template-columns: var(--rail-w) minmax(0, 1fr);
    grid-template-rows: var(--bar-h) minmax(0, 1fr);
    height: 100%;
    overflow: hidden;
    background: var(--md-sys-color-surface);
    /* The rail keeps its grid column when collapsed (narrowed to nothing), so the stage never
       jumps into column one: the prototype's blank-layer bug. */
    transition: grid-template-columns var(--md-sys-motion-duration-long) var(--md-sys-motion-emphasized);
  }
  .shell.rail-collapsed {
    --rail-w: 0px;
  }
  /* mpv follows the player box's edges; an animating column would leave it a frame behind. */
  .shell.is-watching {
    transition: none;
  }

  /* Top app bar: flat on the surface until the page scrolls under it, then tonal. */
  .topbar {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 16px;
    padding-inline: 4px 16px;
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    z-index: 2;
    transition:
      background-color var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard),
      opacity var(--md-sys-motion-duration-long) var(--md-sys-motion-standard);
  }
  .topbar.is-scrolled {
    background: var(--md-sys-color-surface-container);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    padding-right: 8px;
  }
  .mark {
    display: grid;
    color: var(--md-sys-color-primary);
  }
  .wordmark {
    font: 400 22px/28px var(--f-ui);
    color: var(--md-sys-color-on-surface);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .rail {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: center;
    width: 80px;
    overflow: hidden;
    padding: 44px 0 16px;
    background: var(--md-sys-color-surface);
    transition:
      transform var(--md-sys-motion-duration-long) var(--md-sys-motion-emphasized),
      opacity var(--md-sys-motion-duration-medium) var(--md-sys-motion-standard);
  }
  .rail-top,
  .rail-bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }
  .shell.rail-collapsed .rail {
    transform: translateX(-80px);
    opacity: 0;
    pointer-events: none;
  }
  .navitem {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 80px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
    font: 500 12px/16px var(--f-ui);
    letter-spacing: 0.5px;
    -webkit-tap-highlight-color: transparent;
  }
  /* The active indicator: a 56x32 pill that grows out from its centre. */
  .indicator {
    position: relative;
    overflow: hidden;
    isolation: isolate;
    display: grid;
    place-items: center;
    width: 56px;
    height: 32px;
    border-radius: 16px;
  }
  .indicator::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: inherit;
    background: var(--md-sys-color-secondary-container);
    opacity: 0;
    transform: scaleX(0.3);
    transition:
      transform var(--md-sys-motion-duration-medium) var(--md-sys-motion-emphasized),
      opacity var(--md-sys-motion-duration-short) var(--md-sys-motion-standard);
  }
  .indicator::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: inherit;
    background: var(--md-sys-color-on-surface);
    opacity: 0;
    transition: opacity var(--md-sys-motion-duration-short) var(--md-sys-motion-standard);
  }
  .navitem:hover .indicator::after {
    opacity: var(--md-sys-state-hover);
  }
  .navitem:focus-visible {
    outline: none;
  }
  .navitem:focus-visible .indicator::after {
    opacity: var(--md-sys-state-focus);
  }
  .navitem.is-active {
    color: var(--md-sys-color-on-surface);
  }
  .navitem.is-active .indicator {
    color: var(--md-sys-color-on-secondary-container);
  }
  .navitem.is-active .indicator::before {
    opacity: 1;
    transform: none;
  }
  .navitem.is-active .navlabel {
    font-weight: 700;
  }
  /* A download is running. */
  .indicator .dot {
    position: absolute;
    top: 4px;
    right: 14px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--md-sys-color-primary);
    box-shadow: 0 0 0 2px var(--md-sys-color-surface);
  }

  /* Above the page: a server that isn't answering, or an action that failed. Inline, never a modal. */
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 12px clamp(16px, 2.2vw, 26px) 0;
    padding: 8px 8px 8px 14px;
    border-radius: var(--md-sys-shape-md);
    background: var(--md-sys-color-surface-container-high);
    font: 400 14px/20px var(--f-ui);
    letter-spacing: 0.25px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .banner-actions {
    display: flex;
    flex: none;
    gap: 8px;
  }
  .banner.is-alert {
    background: var(--md-sys-color-error-container);
    color: var(--md-sys-color-on-error-container);
  }

  /* The stage is a rounded sheet on the surface, as M3 large-screen layouts do it. */
  .stage {
    overflow-y: auto;
    overscroll-behavior: contain;
    background: var(--md-sys-color-surface);
  }
  /* The watch screen paints its own ground around the player box, which mpv shows through. */
  .stage.is-watching {
    overflow: hidden;
    background: transparent;
  }

  /* Theater mode dims the chrome; full screen removes it. The watch screen sets the mode. */
  :global(:root[data-player-mode="theater"]) .topbar:not(:hover, :focus-within) {
    opacity: 0.34;
  }
  :global(:root[data-player-mode="theater"]) .rail:not(:hover, :focus-within) {
    opacity: 0.3;
  }
  :global(:root[data-player-mode="fullscreen"]) .shell {
    --rail-w: 0px;
    --bar-h: 0px;
  }
  :global(:root[data-player-mode="fullscreen"]) .topbar,
  :global(:root[data-player-mode="fullscreen"]) .rail {
    display: none;
  }

  @media (max-width: 720px) {
    .wordmark,
    .tools :global(.server-name) {
      display: none;
    }
  }

  code {
    font-family: inherit;
    color: var(--ink);
  }
  .zoom-readout {
    position: fixed;
    top: 76px;
    left: 50%;
    z-index: 50;
    transform: translateX(-50%);
    padding: 6px 12px;
    border-radius: var(--r-ctl);
    background: var(--md-sys-color-inverse-surface);
    color: var(--md-sys-color-inverse-on-surface);
    box-shadow: var(--md-sys-elevation-3);
    font: 500 14px/20px var(--f-ui);
    font-variant-numeric: tabular-nums;
  }
</style>
