<script lang="ts" module>
  import { languageList } from "./language";

  function runtime(minutes: number): string {
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return h ? (m ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
  }
</script>

<script lang="ts">
  import { tick, untrack } from "svelte";
  import * as api from "./api";
  import Art from "./Art.svelte";
  import DownloadButton from "./DownloadButton.svelte";
  import { downloadOf } from "./downloads.svelte";
  import { forgetLive, live } from "./live.svelte";
  import EpisodeHover from "./EpisodeHover.svelte";
  import HoverPanel from "./HoverPanel.svelte";
  import Icon from "./Icon.svelte";
  import MediaCard from "./MediaCard.svelte";
  import { HoverIntent } from "./hover.svelte";
  import { markSource } from "./motion";
  import MediaSpecs from "./MediaSpecs.svelte";
  import { scheme, seedFromImage } from "./m3";
  import Rail from "./Rail.svelte";
  import Select from "./Select.svelte";

  let {
    id,
    onBack,
    onPlay,
    onOpen,
    onSignedOut,
    onOpenDownloads,
    onTrailer,
  }: {
    id: string;
    onBack: () => void;
    onPlay: (itemId: string) => void;
    /** A title in More like this was chosen. */
    onOpen: (card: api.Card) => void;
    onSignedOut: () => void;
    onOpenDownloads: () => void;
    /** Play one of the item's trailers on the watch screen. */
    onTrailer: (itemId: string, index: number) => void;
  } = $props();

  /** With yt-dlp a trailer plays in Bloom; without it, it opens in the browser. */
  async function watchTrailer() {
    if (!detail) return;
    actionError = "";
    const inApp = await api.trailersInApp().catch(() => false);
    if (inApp) {
      onTrailer(detail.id, 0);
      return;
    }
    try {
      await api.openTrailer(detail.id, 0);
    } catch (e) {
      actionError = failed(e);
    }
  }

  let detail = $state<api.ItemDetail | null>(null);
  /** The page takes its colours from the title's artwork (Material 3 dynamic colour): a scheme
   *  worked out from the poster's dominant colour, scoped to this page. */
  let tonalStyle = $state("");
  $effect(() => {
    const art = detail?.poster ?? detail?.backdrop;
    if (!art) return;
    let stale = false;
    seedFromImage(api.imageUrl(art, 160)).then((seed) => {
      if (stale || !seed) return;
      const dark = document.documentElement.dataset.scheme !== "light";
      tonalStyle = Object.entries(scheme(seed, dark))
        .map(([role, hex]) => `--md-sys-color-${role.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}:${hex}`)
        .join(";");
    });
    return () => (stale = true);
  });
  let error = $state("");
  let favorite = $state(false);
  let played = $state(false);
  let busy = $state(false);
  let actionError = $state("");
  let expanded = $state(false);
  let logoBroken = $state(false);
  let similar = $state<api.Card[] | null>(null);
  /** The collections this title is in. */
  let collections = $state<api.CollectionRow[]>([]);
  /** For a collection: its titles. */
  let collectionCards = $state<api.Card[] | null>(null);

  let seasonId = $state<string | null>(null);
  let episodes = $state<api.EpisodePage | null>(null);
  let episodesError = $state("");
  let episodesLoading = $state(false);
  let episodeError = $state("");
  let newestFirst = $state(false);
  /** The episode whose "more" menu is open. */
  let menuFor = $state<string | null>(null);
  let episodesSection = $state<HTMLElement>();
  const epHover = new HoverIntent();
  const epAnchors: Record<string, HTMLElement> = {};

  // The rendered width at 100% zoom, for asking Rust for the right image size.
  const backdropWidth = Math.min(window.innerWidth, 1920);

  let isSeries = $derived(detail?.kind === "Series");
  let isCollection = $derived(detail?.kind === "BoxSet");
  let seasonIndex = $derived(detail ? detail.seasons.findIndex((s) => s.id === seasonId) : -1);
  let season = $derived(detail?.seasons[seasonIndex] ?? null);
  let shownEpisodes = $derived(episodes ? (newestFirst ? [...episodes.episodes].reverse() : episodes.episodes) : []);
  let seasonOptions = $derived(
    (detail?.seasons ?? []).map((s) => ({
      value: s.id,
      label: s.name,
      detail: `${s.episodeCount} ${s.episodeCount === 1 ? "episode" : "episodes"}`,
    })),
  );

  let meta = $derived.by(() => {
    if (!detail) return [];
    const parts: string[] = [];
    if (detail.year) {
      if (!isSeries) parts.push(String(detail.year));
      else if (detail.status === "Continuing") parts.push(`${detail.year}–`);
      else if (detail.endYear && detail.endYear !== detail.year) parts.push(`${detail.year}–${detail.endYear}`);
      else parts.push(String(detail.year));
    }
    if (isSeries) {
      const seasons = detail.seasons.length;
      if (seasons) parts.push(`${seasons} ${seasons === 1 ? "season" : "seasons"}`);
      if (detail.episodeCount) {
        parts.push(`${detail.episodeCount} episodes`);
        if (detail.unplayedCount != null) parts.push(`${detail.episodeCount - detail.unplayedCount} of ${detail.episodeCount} watched`);
      }
    } else if (detail.runtimeMinutes) {
      parts.push(runtime(detail.runtimeMinutes));
    }
    if (detail.communityRating) parts.push(`${detail.communityRating.toFixed(1)}/10`);
    return parts;
  });

  function failed(e: unknown): string {
    if (api.isSignedOut(e)) {
      onSignedOut();
      return "";
    }
    return api.message(e);
  }

  async function load() {
    error = "";
    try {
      const loaded = await api.itemDetail(id);
      detail = loaded;
      favorite = loaded.favorite;
      played = loaded.played;
      // Open the season the next episode up is in.
      seasonId = loaded.play?.seasonId ?? loaded.seasons[0]?.id ?? null;
      api
        .similar(loaded.id)
        .then((cards) => (similar = cards))
        .catch(() => (similar = []));
      if (loaded.kind === "BoxSet") {
        api
          .collectionItems(loaded.id)
          .then((cards) => (collectionCards = cards))
          .catch((e) => {
            collectionCards = [];
            actionError = failed(e);
          });
      } else {
        // Quiet when it fails: the rows are extra, and the page stands without them.
        api
          .itemCollections(loaded.id)
          .then((rows) => (collections = rows))
          .catch(() => (collections = []));
      }
    } catch (e) {
      error = failed(e);
    }
  }

  $effect(() => {
    untrack(load);
  });

  let request = 0;
  async function loadEpisodes() {
    const season = seasonId;
    if (!season) return;
    const mine = ++request;
    episodesLoading = true;
    episodesError = "";
    try {
      const result = await api.seasonEpisodes(id, season);
      if (mine === request) episodes = result;
    } catch (e) {
      if (mine === request) episodesError = failed(e);
    } finally {
      if (mine === request) episodesLoading = false;
    }
  }

  $effect(() => {
    void seasonId;
    if (isSeries) untrack(loadEpisodes);
  });

  function chooseSeason(next: string, scroll = false) {
    if (next === seasonId) return;
    seasonId = next;
    menuFor = null;
    episodeError = "";
    if (scroll) episodesSection?.scrollIntoView({ block: "start" });
  }

  async function toggle(kind: "favorite" | "played") {
    if (!detail || busy) return;
    busy = true;
    actionError = "";
    forgetLive(detail.id);
    try {
      if (kind === "favorite") favorite = await api.setFavorite(detail.id, !favorite);
      else played = await api.setPlayed(detail.id, !played);
    } catch (e) {
      actionError = failed(e);
    } finally {
      busy = false;
    }
  }

  async function toggleEpisodePlayed(ep: api.Episode) {
    menuFor = null;
    episodeError = "";
    forgetLive(ep.id);
    try {
      const now = await api.setPlayed(ep.id, !ep.played);
      ep.played = now;
      if (now) ep.progress = null;
    } catch (e) {
      episodeError = failed(e);
    }
  }

  // The server's live updates, laid over what the page loaded: the title's own watched mark and
  // favourite, and each episode's.
  $effect(() => {
    if (!detail) return;
    const own = live.get(detail.id);
    if (own) {
      favorite = own.favorite;
      played = own.played;
    }
    for (const ep of episodes?.episodes ?? []) {
      const change = live.get(ep.id);
      if (!change) continue;
      ep.played = change.played;
      ep.progress = change.played ? null : change.progress;
    }
  });

  // --- a whole season at once

  let seasonMarking = $state(false);
  let seasonWatched = $derived(!!episodes?.episodes.length && episodes.episodes.every((ep) => ep.played));

  /** Marks every episode of the season shown, through the season itself. */
  async function toggleSeasonPlayed() {
    if (!detail || !seasonId || !episodes || seasonMarking) return;
    const marking = seasonId;
    seasonMarking = true;
    episodeError = "";
    try {
      const played = await api.setPlayed(marking, !seasonWatched);
      if (seasonId !== marking || !episodes) return;
      for (const ep of episodes.episodes) {
        forgetLive(ep.id);
        ep.played = played;
        if (played) ep.progress = null;
      }
      if (season) {
        const unplayed = played ? 0 : season.episodeCount;
        if (detail.unplayedCount != null) detail.unplayedCount = Math.max(0, detail.unplayedCount + unplayed - season.unplayed);
        season.unplayed = unplayed;
      }
    } catch (e) {
      episodeError = failed(e);
    } finally {
      seasonMarking = false;
    }
  }

  // --- downloads

  let seasonQueuing = $state(false);
  /** Every episode of the season shown is in Downloads, finished or not. */
  let seasonListed = $derived(!!episodes?.episodes.length && episodes.episodes.every((ep) => downloadOf(ep.id)));

  async function downloadSeason() {
    if (!detail || !seasonId || seasonQueuing) return;
    if (seasonListed) {
      onOpenDownloads();
      return;
    }
    seasonQueuing = true;
    episodeError = "";
    try {
      await api.downloadSeason(detail.id, seasonId);
    } catch (e) {
      episodeError = failed(e);
    } finally {
      seasonQueuing = false;
    }
  }

  async function toggleEpisodeDownload(ep: api.Episode) {
    menuFor = null;
    episodeError = "";
    const listed = downloadOf(ep.id);
    try {
      if (listed) await api.removeDownload(listed.id);
      else await api.downloadItem(ep.id);
    } catch (e) {
      episodeError = failed(e);
    }
  }

  function languagesOf(ep: api.Episode): string[] {
    const parts: string[] = [];
    if (ep.audioLanguages.length) parts.push(languageList(ep.audioLanguages));
    if (ep.hasSubtitles) parts.push("Subtitles");
    return parts;
  }

  $effect(() => {
    if (!menuFor) return;
    tick().then(() => document.querySelector<HTMLElement>(".ep-menu [role='menuitem']")?.focus());
    const onPointer = (e: PointerEvent) => {
      if (!(e.target as Element | null)?.closest?.(".ep-menu, .ep-more")) menuFor = null;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      const opener = document.querySelector<HTMLElement>(".ep-more[aria-expanded='true']");
      menuFor = null;
      opener?.focus();
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  });
</script>

<div class="item tonal" style={tonalStyle}>
  {#if error}
    <div class="pad">
      <button class="btn" onclick={onBack}><Icon name="chevl" size={14} />Back</button>
      <div class="page-notice" role="alert">
        <h2>Couldn't load this</h2>
        <p>{error}</p>
        <button class="btn" onclick={load}>Try again</button>
      </div>
    </div>
  {:else if !detail}
    <div class="hero is-loading skel" aria-busy="true" aria-label="Loading" data-morph-target>
      <div class="hero-content"><div class="sk-lines"><span></span><span></span><span></span></div></div>
    </div>
  {:else}
    <section class="hero" data-morph-target>
      <div class="hero-art" aria-hidden="true">
        {#if detail.backdrop}
          <img class="backdrop" src={api.imageUrl(detail.backdrop, backdropWidth)} alt="" decoding="async" />
        {/if}
        <div class="veil"></div>
      </div>
      <button class="btn back" onclick={onBack}><Icon name="chevl" size={14} />Back</button>

      <div class="hero-content">
        {#if detail.logo && !logoBroken}
          <h1 class="logo-wrap">
            <img class="logo" src={api.imageUrl(detail.logo, 460)} alt={detail.title} onerror={() => (logoBroken = true)} />
          </h1>
        {:else}
          <h1 class="hero-title">{detail.title}</h1>
        {/if}
        <div class="meta-line">
          {#if detail.officialRating}<span class="tag">{detail.officialRating}</span>{/if}
          {#each meta as part, i (i)}
            {#if i > 0 || detail.officialRating}<span class="sep"></span>{/if}<span>{part}</span>
          {/each}
        </div>
        {#if detail.genres.length}<p class="genres">{detail.genres.join(", ")}</p>{/if}
        <div class="actions">
          {#if detail.play}
            {@const target = detail.play}
            <button class="btn btn-primary" onclick={() => onPlay(target.itemId)}>
              <Icon name="play" size={16} />{target.label}
            </button>
          {/if}
          {#if detail.trailers?.length}
            <button class="btn" onclick={watchTrailer}><Icon name="theater" size={16} />Trailer</button>
          {/if}
          {#if !isSeries && !isCollection}
            <button class="btn" aria-pressed={played} disabled={busy} onclick={() => toggle("played")}>
              <Icon name="check" size={16} />{played ? "Watched" : "Mark watched"}
            </button>
            <DownloadButton itemId={detail.id} {onOpenDownloads} onError={(e) => (actionError = failed(e))} />
          {/if}
          <button class="btn" aria-pressed={favorite} disabled={busy} onclick={() => toggle("favorite")}>
            <Icon name="heart" size={16} />{favorite ? "Favorited" : "Favorite"}
          </button>
        </div>
        {#if actionError}<p class="inline-error" role="alert">{actionError}</p>{/if}
      </div>
    </section>

    <section class="about" aria-label="About">
      <div class="about-text">
        {#if detail.tagline && expanded}<p class="tagline">{detail.tagline}</p>{/if}
        {#if detail.overview}<p class="synopsis" class:is-clamped={!expanded}>{detail.overview}</p>{/if}
        {#if expanded && (detail.studios.length || detail.status)}
          <dl class="facts">
            {#if detail.studios.length}<div><dt>Studio</dt><dd>{detail.studios.join(", ")}</dd></div>{/if}
            {#if detail.status}<div><dt>Status</dt><dd>{detail.status === "Continuing" ? "Still airing" : detail.status}</dd></div>{/if}
          </dl>
        {/if}
        {#if detail.overview || detail.tagline || detail.studios.length}
          <button class="more" aria-expanded={expanded} onclick={() => (expanded = !expanded)}>
            {expanded ? "Fewer details" : "More details"}
          </button>
        {/if}
      </div>
      <dl class="facts">
        {#if detail.audioLanguages.length}<div><dt>Audio</dt><dd>{languageList(detail.audioLanguages, 8)}</dd></div>{/if}
        {#if detail.subtitleLanguages.length}<div><dt>Subtitles</dt><dd>{languageList(detail.subtitleLanguages, 8)}</dd></div>{/if}
        {#if detail.officialRating}<div><dt>Age rating</dt><dd>{detail.officialRating}</dd></div>{/if}
      </dl>
    </section>

    {#if isCollection}
      <section class="section" aria-label="In this collection">
        <div class="section-head">
          <h2 class="section-title">In this collection</h2>
          {#if collectionCards?.length}
            <span class="eyebrow">{collectionCards.length} {collectionCards.length === 1 ? "title" : "titles"}, oldest first</span>
          {/if}
        </div>
        {#if !collectionCards}
          <div class="media-grid skel" aria-busy="true" aria-label="Loading the collection">
            {#each [0, 1, 2, 3] as i (i)}<div class="sk-card"></div>{/each}
          </div>
        {:else}
          <div class="media-grid stagger">
            {#each collectionCards as card (card.id)}
              <MediaCard {card} shape="poster" width={280} fluid onActivate={onOpen} {onPlay} />
            {/each}
          </div>
        {/if}
      </section>
    {/if}

    {#if isSeries}
      <section class="section" aria-label="Episodes" bind:this={episodesSection}>
        <div class="section-head">
          {#if detail.seasons.length > 1}
            <Select look="heading" label="Season" value={seasonId ?? ""} options={seasonOptions} onChange={(next) => chooseSeason(next)} />
          {:else}
            <h2 class="section-title">{season?.name ?? "Episodes"}</h2>
          {/if}
          {#if season}
            <span class="eyebrow">
              {season.episodeCount} {season.episodeCount === 1 ? "episode" : "episodes"}{season.unplayed ? `, ${season.unplayed} unwatched` : ""}
            </span>
          {/if}
          <button class="btn sort" onclick={() => (newestFirst = !newestFirst)}>
            <Icon name="sort" size={14} />{newestFirst ? "Newest first" : "Oldest first"}
          </button>
          {#if season}
            <button class="btn" aria-pressed={seasonWatched} disabled={seasonMarking || !episodes} onclick={toggleSeasonPlayed}>
              <Icon name="check" size={14} />{seasonWatched ? "Mark season unwatched" : "Mark season watched"}
            </button>
            <button class="btn" disabled={seasonQueuing || !episodes} onclick={downloadSeason}>
              <Icon name={seasonListed ? "check" : "download"} size={14} />{seasonListed ? "Season in Downloads" : "Download season"}
            </button>
          {/if}
        </div>

        {#if episodeError}<p class="inline-error" role="alert">{episodeError}</p>{/if}
        {#if episodesError}
          <div class="page-notice" role="alert">
            <h2>Couldn't load the episodes</h2>
            <p>{episodesError}</p>
            <button class="btn" onclick={loadEpisodes}>Try again</button>
          </div>
        {:else if !episodes}
          <div class="ep-grid skel" aria-busy="true" aria-label="Loading episodes">
            {#each [0, 1, 2, 3, 4, 5] as i (i)}<div class="ep is-loading"><span class="ep-shot"></span><span class="sk-line"></span></div>{/each}
          </div>
        {:else}
          <div class="ep-grid stagger" aria-busy={episodesLoading}>
            {#each shownEpisodes as ep (ep.id)}
              {@const code = ep.number != null ? `E${ep.number}` : null}
              {@const upNext = detail.play?.itemId === ep.id}
              {@const langs = languagesOf(ep)}
              <div class="ep">
                <button
                  class="ep-main"
                  bind:this={epAnchors[ep.id]}
                  onpointerenter={(e) => epHover.enter(ep.id, e)}
                  onpointerleave={epHover.leave}
                  onclick={() => {
                    epHover.close();
                    markSource(epAnchors[ep.id]?.querySelector(".ep-shot"));
                    onPlay(ep.id);
                  }}
                >
                  <span class="ep-shot">
                    <Art image={ep.image} title={ep.title} width={320} />
                    <span class="ep-play" aria-hidden="true"><Icon name="play" size={20} /></span>
                    {#if upNext}<span class="ep-badge">{detail.play?.resume ? "Continue" : "Up next"}</span>{/if}
                    {#if ep.runtimeMinutes}<span class="ep-dur">{runtime(ep.runtimeMinutes)}</span>{/if}
                    {#if ep.progress}<span class="ep-progress"><span style:width="{Math.round(ep.progress * 100)}%"></span></span>{/if}
                  </span>
                  <span class="ep-series">{detail.title}</span>
                  <span class="ep-title">{#if code}<span class="ep-code">{code}</span>{/if}<span>{ep.title}</span></span>
                </button>
                {#if epHover.key === ep.id && epAnchors[ep.id]}
                  <HoverPanel
                    anchor={epAnchors[ep.id]}
                    fit="cover"
                    label={ep.title}
                    onEnter={(e) => epHover.enter(ep.id, e)}
                    onLeave={epHover.leave}
                    onClose={epHover.close}
                    onActivate={() => {
                      epHover.close();
                      onPlay(ep.id);
                    }}
                  >
                    <EpisodeHover
                      seriesTitle={detail.title}
                      {code}
                      title={ep.title}
                      premiereDate={ep.premiereDate}
                      overview={ep.overview}
                      played={ep.played}
                      resume={!!ep.progress}
                      onPlay={() => {
                        epHover.close();
                        onPlay(ep.id);
                      }}
                      onTogglePlayed={() => toggleEpisodePlayed(ep)}
                    />
                  </HoverPanel>
                {/if}
                <div class="ep-foot">
                  <span class="ep-langs">
                    {#each langs as part, i (i)}{#if i > 0}<span class="sep"></span>{/if}<span>{part}</span>{/each}
                    {#if ep.played}
                      {#if langs.length}<span class="sep"></span>{/if}<span class="ep-watched"><Icon name="check" size={12} />Watched</span>
                    {/if}
                    {#if downloadOf(ep.id)?.status === "done"}
                      {#if langs.length || ep.played}<span class="sep"></span>{/if}<span class="ep-watched"><Icon name="download" size={12} />Downloaded</span>
                    {/if}
                  </span>
                  <div class="ep-menu-anchor">
                    <button
                      class="ep-more"
                      aria-label="More for {code ?? ep.title}"
                      aria-haspopup="menu"
                      aria-expanded={menuFor === ep.id}
                      onclick={() => (menuFor = menuFor === ep.id ? null : ep.id)}
                    >
                      <Icon name="more" size={16} />
                    </button>
                    {#if menuFor === ep.id}
                      {@const listed = downloadOf(ep.id)}
                      <div class="ep-menu" role="menu" aria-label="{code ?? ep.title}">
                        <button class="ep-menu-item" role="menuitem" onclick={() => toggleEpisodePlayed(ep)}>
                          {ep.played ? "Mark as unwatched" : "Mark as watched"}
                        </button>
                        <button class="ep-menu-item" role="menuitem" onclick={() => toggleEpisodeDownload(ep)}>
                          {listed ? (listed.status === "done" ? "Delete download" : "Cancel download") : "Download episode"}
                        </button>
                      </div>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>

          {#if detail.seasons.length > 1}
            <nav class="season-nav" aria-label="Seasons">
              {#if seasonIndex > 0}
                {@const prev = detail.seasons[seasonIndex - 1]}
                <button class="btn" onclick={() => chooseSeason(prev.id, true)}><Icon name="chevl" size={14} />{prev.name}</button>
              {:else}
                <span></span>
              {/if}
              {#if seasonIndex >= 0 && seasonIndex < detail.seasons.length - 1}
                {@const next = detail.seasons[seasonIndex + 1]}
                <button class="btn" onclick={() => chooseSeason(next.id, true)}>{next.name}<Icon name="chevr" size={14} /></button>
              {/if}
            </nav>
          {/if}
        {/if}
      </section>
    {/if}

    {#if detail.people.length}
      <div class="section">
        <Rail title="Cast">
          {#each detail.people as person, i (`${person.id}-${i}`)}
            <div class="person">
              <span class="person-art"><Art image={person.image} title={person.name} width={120} /></span>
              <span class="person-name">{person.name}</span>
              {#if person.role}<span class="person-role">{person.role}</span>{/if}
            </div>
          {/each}
        </Rail>
      </div>
    {/if}

    {#if detail.media}
      <section class="section" aria-label="Media info">
        <h2 class="section-title media-title">Media info</h2>
        <MediaSpecs media={detail.media} studios={detail.studios} />
      </section>
    {/if}

    {#each collections as row (row.id)}
      <div class="section">
        <Rail title="Part of {row.title}">
          {#each row.cards as card (card.id)}
            <MediaCard {card} shape="poster" width={240} onActivate={onOpen} {onPlay} />
          {/each}
        </Rail>
        <button
          class="btn collection-link"
          onclick={() => onOpen({ id: row.id, kind: "BoxSet", title: row.title, meta: null, image: null, progress: null })}
        >
          View the collection<Icon name="chevr" size={12} />
        </button>
      </div>
    {/each}

    {#if similar?.length}
      <div class="section">
        <Rail title="More like this">
          {#each similar as card (card.id)}
            <MediaCard {card} shape="poster" width={240} onActivate={onOpen} {onPlay} />
          {/each}
        </Rail>
      </div>
    {/if}
  {/if}
</div>

<style>
  .item {
    --gutter: clamp(16px, 2.2vw, 26px);
    padding-bottom: 56px;
  }
  .pad {
    padding: 22px var(--gutter);
  }

  /* The hero follows the Home spotlight: a full-bleed backdrop under a veil fading to the page
     ground from the left and the bottom (and, lightly, the top for Back), text on the ground. */
  .hero {
    position: relative;
    min-height: clamp(420px, 62vh, 680px);
    display: flex;
    align-items: flex-end;
    overflow: hidden;
  }
  .hero.is-loading {
    background: var(--surface);
  }
  .hero-art {
    position: absolute;
    inset: 0;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: 70% 22%;
  }
  .veil {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(180deg, color-mix(in oklab, var(--ground) 70%, transparent) 0, transparent 110px),
      linear-gradient(
        90deg,
        var(--ground) 0%,
        color-mix(in oklab, var(--ground) 86%, transparent) 28%,
        color-mix(in oklab, var(--ground) 30%, transparent) 58%,
        transparent 78%
      ),
      linear-gradient(0deg, var(--ground) 0%, color-mix(in oklab, var(--ground) 60%, transparent) 20%, transparent 46%);
  }
  .back {
    position: absolute;
    top: 16px;
    left: var(--gutter);
    z-index: 1;
  }
  .hero-content {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    max-width: min(680px, 100%);
    padding: 72px var(--gutter) clamp(24px, 4vh, 40px);
  }
  .logo-wrap,
  .hero-title {
    margin: 0 0 6px;
  }
  .logo {
    display: block;
    max-width: min(440px, 80%);
    max-height: clamp(72px, 14vh, 150px);
    object-fit: contain;
    object-position: left bottom;
  }
  .hero-title {
    font-stretch: 118%;
    font-weight: 600;
    letter-spacing: -0.02em;
    font-size: clamp(2rem, 4vw, 3.2rem);
    line-height: 1.02;
    overflow-wrap: anywhere;
  }
  .meta-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }
  .sep {
    flex: none;
    width: 1px;
    height: 11px;
    background: var(--line);
  }
  .tag {
    padding: 1px 6px;
    border: 1px solid var(--line);
    border-radius: 5px;
    font-size: 11.5px;
    white-space: nowrap;
  }
  .genres {
    margin: 0;
    font-size: 13.5px;
    color: var(--ink-2);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 8px;
  }
  /* Global, so the Download button (its own component) matches the others in the row. */
  .actions :global(.btn) {
    height: 40px;
    padding: 0 16px;
    font-size: 14px;
  }
  .actions :global(.btn[aria-pressed="true"]) {
    color: var(--accent);
    border-color: color-mix(in oklab, var(--accent) 42%, var(--line));
  }
  .collection-link {
    margin-top: 12px;
  }
  .inline-error {
    margin: 0;
    font-size: 13px;
    color: var(--alert);
  }

  .about {
    display: grid;
    grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr);
    gap: 18px 56px;
    margin-inline: var(--gutter);
    padding-block: 6px 26px;
    border-bottom: 1px solid var(--line-soft);
  }
  .about-text {
    min-width: 0;
  }
  .tagline {
    margin: 0 0 10px;
    font-size: 14px;
    color: var(--ink);
  }
  .synopsis {
    margin: 0;
    white-space: pre-line;
    max-width: 72ch;
    font-size: 14px;
    line-height: 1.65;
    color: var(--ink);
  }
  .synopsis.is-clamped {
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .more {
    margin-top: 10px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }
  .more:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .facts {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
  }
  .about-text .facts {
    margin-top: 14px;
  }
  .facts div {
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr);
    gap: 12px;
    font-size: 13.5px;
    line-height: 1.5;
  }
  .facts dt {
    color: var(--ink-3);
  }
  .facts dd {
    margin: 0;
    color: var(--ink-2);
  }

  .section {
    margin: 30px var(--gutter) 0;
  }
  .section-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
    margin-bottom: 14px;
  }
  .section-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.005em;
  }
  .media-title {
    margin-bottom: 14px;
  }
  .sort {
    margin-left: auto;
  }

  .ep-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 26px 16px;
  }
  .ep-grid[aria-busy="true"] {
    opacity: 0.6;
  }
  .ep {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .ep-main {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .ep-shot {
    position: relative;
    display: block;
    aspect-ratio: 16 / 9;
    margin-bottom: 8px;
  }
  .ep-shot > :global(.art) {
    position: absolute;
    inset: 0;
  }
  .ep-main:hover .ep-shot > :global(.art) {
    filter: brightness(1.07);
  }
  .ep-main:focus-visible {
    outline: none;
  }
  .ep-main:focus-visible .ep-shot {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: var(--r);
  }
  .ep-play {
    position: absolute;
    left: 50%;
    top: 50%;
    z-index: 2;
    width: 46px;
    height: 46px;
    display: grid;
    place-items: center;
    padding-left: 3px;
    border-radius: 50%;
    background: rgba(18, 20, 19, 0.62);
    color: #f2f4f3;
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.9);
    transition: opacity 0.18s var(--ease), transform 0.18s var(--ease);
  }
  .ep-main:hover .ep-play,
  .ep-main:focus-visible .ep-play {
    opacity: 1;
    transform: translate(-50%, -50%);
  }
  .ep-dur,
  .ep-badge {
    position: absolute;
    z-index: 2;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(18, 20, 19, 0.84);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .ep-dur {
    right: 6px;
    bottom: 6px;
    color: #edefee;
  }
  .ep-badge {
    left: 6px;
    top: 6px;
    color: var(--accent-media);
  }
  .ep-progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 2;
    height: 3px;
    overflow: hidden;
    border-radius: 0 0 var(--r) var(--r);
    background: rgba(18, 20, 19, 0.4);
  }
  .ep-progress span {
    display: block;
    height: 100%;
    background: var(--accent-media);
  }
  .ep-series {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .ep-title {
    display: flex;
    gap: 7px;
    font-size: 14px;
    font-weight: 500;
    line-height: 1.3;
  }
  .ep-code {
    flex: none;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .ep-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    margin-top: 4px;
  }
  .ep-langs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
    min-width: 0;
    font-size: 12.5px;
    color: var(--ink-3);
  }
  .ep-watched {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--good);
  }
  .ep-menu-anchor {
    position: relative;
    margin-left: auto;
  }
  .ep-more {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--ink-2);
    cursor: pointer;
  }
  .ep-more:hover,
  .ep-more[aria-expanded="true"] {
    background: var(--surface);
    color: var(--ink);
  }
  .ep-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 20;
    min-width: 180px;
    padding: 6px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: var(--r);
    box-shadow: var(--shadow);
    animation: pop 0.2s var(--ease) both;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  .ep-menu-item {
    display: block;
    width: 100%;
    padding: 7px 8px;
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--ink-2);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .ep-menu-item:hover,
  .ep-menu-item:focus-visible {
    outline: none;
    background: var(--surface-2);
    color: var(--ink);
  }
  /* Placeholders shaped like the tiles; the scan line over them is `.skel` in app.css. */
  .ep.is-loading .ep-shot {
    border-radius: var(--r);
    background: var(--surface);
  }
  .sk-line {
    width: 70%;
    height: 12px;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .sk-lines {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(420px, 100%);
  }
  .sk-lines span {
    height: 14px;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .sk-lines span:first-child {
    width: 70%;
    height: 44px;
  }

  .season-nav {
    display: flex;
    justify-content: space-between;
    margin-top: 26px;
    padding-top: 14px;
    border-top: 1px solid var(--line-soft);
  }

  .person {
    flex: none;
    width: 120px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .person-art {
    position: relative;
    width: 100%;
    aspect-ratio: 1;
    margin-bottom: 5px;
  }
  .person-art > :global(.art) {
    position: absolute;
    inset: 0;
  }
  .person-name {
    font-size: 12.5px;
    font-weight: 500;
    line-height: 1.3;
  }
  .person-role {
    font-size: 11.5px;
    line-height: 1.3;
    color: var(--ink-3);
  }

  @media (max-width: 860px) {
    .about {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
